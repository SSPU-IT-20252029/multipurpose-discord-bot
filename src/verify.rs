//! Email verification business logic.
//!
//! Parity target: Go `internal/verify/service.go`. `Start` and `Confirm`
//! replicate the exact validation order and semantics, including email
//! normalization, domain strict-match, rate limiting, hashed codes with
//! constant-time comparison, and role resolution.

use crate::i18n::{self, Locale, Translations};
use crate::mailer::MailerError;
use crate::store::{PendingCode, Store, StoreError};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

const NANOS_PER_SEC: i64 = 1_000_000_000;

/// Parity: Go `verify.Mailer` interface. Crate-internal trait used via
/// generics only; `async fn` is fine here (both implementors' futures are
/// `Send`), so the `async_fn_in_trait` lint is intentionally allowed.
#[allow(async_fn_in_trait)]
pub trait Mailer: Send + Sync {
    async fn send_code(
        &self,
        to: &str,
        subject: &str,
        code: &str,
        ttl: Duration,
        locale: Locale,
    ) -> Result<(), MailerError>;
}

impl Mailer for crate::mailer::Mailer {
    async fn send_code(
        &self,
        to: &str,
        subject: &str,
        code: &str,
        ttl: Duration,
        locale: Locale,
    ) -> Result<(), MailerError> {
        crate::mailer::Mailer::send_code(self, to, subject, code, ttl, locale).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("guild is not set up")]
    MissingConfig,
    #[error("email domain is not allowed")]
    InvalidDomain,
    #[error("no rule matches this email")]
    NotActive,
    #[error("email is already bound to another user")]
    EmailAlreadyUsed,
    #[error("verification limit exceeded")]
    RateLimited,
    #[error("no pending code found")]
    NoPending,
    #[error("code expired")]
    Expired,
    #[error("too many attempts")]
    TooManyAttempts,
    #[error("wrong code, {remaining} attempt(s) left")]
    WrongCode { remaining: i64 },
    #[error("failed to send email")]
    SendFailed,
    #[error("store error: {0}")]
    Store(#[from] StoreError),
}

impl VerifyError {
    /// Port of Go `localizeError` — the interaction layer calls this with the
    /// user's locale to produce the ephemeral message.
    pub fn localize(&self, t: Translations) -> String {
        match self {
            VerifyError::MissingConfig => t.err_missing_config.to_string(),
            VerifyError::InvalidDomain => t.err_invalid_domain.to_string(),
            VerifyError::NotActive => t.err_not_active.to_string(),
            VerifyError::EmailAlreadyUsed => t.err_email_already_used.to_string(),
            VerifyError::RateLimited => t.err_rate_limited.to_string(),
            VerifyError::NoPending => t.err_no_pending.to_string(),
            VerifyError::Expired => t.err_expired.to_string(),
            VerifyError::TooManyAttempts => t.err_too_many_attempts.to_string(),
            VerifyError::WrongCode { remaining } => {
                i18n::subst(t.err_wrong_code_fmt, &[&remaining.to_string()])
            }
            VerifyError::SendFailed => t.err_send_failed.to_string(),
            // Go falls through to err.Error() for anything unrecognized.
            VerifyError::Store(e) => e.to_string(),
        }
    }
}

pub struct VerifyService<M: Mailer> {
    store: Store,
    mailer: M,
    now: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl<M: Mailer> VerifyService<M> {
    pub fn new(store: Store, mailer: M) -> Self {
        Self {
            store,
            mailer,
            now: Arc::new(now_unix_secs),
        }
    }

    /// Constructor with an injectable clock (used by tests). Parity: Go's
    /// `Service.Now func() time.Time`.
    pub fn with_clock(
        store: Store,
        mailer: M,
        now: impl Fn() -> i64 + Send + Sync + 'static,
    ) -> Self {
        Self {
            store,
            mailer,
            now: Arc::new(now),
        }
    }

    fn now(&self) -> i64 {
        (self.now)()
    }

    /// Parity: Go `Service.Start` — exact ordering.
    pub async fn start(
        &self,
        guild_id: &str,
        discord_id: &str,
        email: &str,
        locale: Locale,
    ) -> Result<(), VerifyError> {
        // Normalize first (Go: strings.ToLower(strings.TrimSpace(email))).
        let email = email.trim().to_ascii_lowercase();

        // 1. Guild config present and domain set.
        let cfg = match self.store.get_guild_config(guild_id)? {
            Some(c) if !c.domain.is_empty() => c,
            _ => return Err(VerifyError::MissingConfig),
        };

        // 2. Domain strict-match: exactly one '@', right side equals cfg.domain.
        let parts: Vec<&str> = email.split('@').collect();
        if parts.len() != 2 || parts[1] != cfg.domain.as_str() {
            return Err(VerifyError::InvalidDomain);
        }

        // 3. Early role pre-check (rejects inactive emails before sending).
        self.resolve_role(guild_id, &email, &cfg.mode)?;

        let now = self.now();

        // 4. Email squatting.
        if let Some(existing) = self.store.get_verified_by_email(guild_id, &email)?
            && existing.discord_id != discord_id
        {
            return Err(VerifyError::EmailAlreadyUsed);
        }

        // 5. Rate limit (window stored as nanoseconds).
        let window_secs = cfg.rate_limit_window_ns / NANOS_PER_SEC;
        let sent = self
            .store
            .count_sends_since(guild_id, discord_id, now - window_secs)?;
        if sent >= cfg.rate_limit_count {
            return Err(VerifyError::RateLimited);
        }

        // 6. Generate + persist hashed code.
        let code = generate_code();
        let ttl_ns = cfg.code_ttl_ns.max(0);
        let pending = PendingCode {
            guild_id: guild_id.to_string(),
            discord_id: discord_id.to_string(),
            email: email.clone(),
            code_hash: hash(&code),
            expires_at: now + ttl_ns / NANOS_PER_SEC,
            attempts: 0,
        };
        self.store.upsert_pending(&pending)?;

        // 7. Send. On failure the pending row remains (Go parity).
        if let Err(e) = self
            .mailer
            .send_code(
                &email,
                &cfg.subject,
                &code,
                Duration::from_nanos(ttl_ns as u64),
                locale,
            )
            .await
        {
            tracing::error!(error = %e, guild_id, discord_id, "failed to send verification email");
            return Err(VerifyError::SendFailed);
        }

        // 8. Log the send (with 2h prune inside).
        self.store.log_send(guild_id, discord_id, now)?;
        Ok(())
    }

    /// Parity: Go `Service.Confirm` — returns the role IDs to assign.
    pub async fn confirm(
        &self,
        guild_id: &str,
        discord_id: &str,
        code: &str,
    ) -> Result<Vec<String>, VerifyError> {
        // 1. Pending must exist.
        let pending = match self.store.get_pending(guild_id, discord_id)? {
            Some(p) => p,
            None => return Err(VerifyError::NoPending),
        };

        // 2. Config must exist.
        let cfg = match self.store.get_guild_config(guild_id)? {
            Some(c) => c,
            None => return Err(VerifyError::MissingConfig),
        };

        // 3. Expiry.
        let now = self.now();
        if now > pending.expires_at {
            let _ = self.store.delete_pending(guild_id, discord_id);
            return Err(VerifyError::Expired);
        }

        // 4. Constant-time hash comparison of the space-stripped input.
        if !constant_time_eq(&hash(&normalize_code(code)), &pending.code_hash) {
            let attempts = self.store.increment_attempts(guild_id, discord_id)?;
            if attempts >= cfg.max_attempts {
                let _ = self.store.delete_pending(guild_id, discord_id);
                return Err(VerifyError::TooManyAttempts);
            }
            return Err(VerifyError::WrongCode {
                remaining: cfg.max_attempts - attempts,
            });
        }

        // 5. Re-resolve the role; a failure deletes the pending code (Go parity).
        let role_id = match self.resolve_role(guild_id, &pending.email, &cfg.mode) {
            Ok(r) => r,
            Err(e) => {
                let _ = self.store.delete_pending(guild_id, discord_id);
                return Err(e);
            }
        };

        // 6. Persist verification + 7. clear the pending code.
        self.store
            .set_verified(guild_id, discord_id, &pending.email, &role_id)?;
        let _ = self.store.delete_pending(guild_id, discord_id);

        // 8. Role list: primary + optional default (different from primary).
        let mut roles = vec![role_id];
        if !cfg.default_role_id.is_empty() && cfg.default_role_id != roles[0] {
            roles.push(cfg.default_role_id);
        }
        Ok(roles)
    }

    /// Parity: Go `resolveRole`. REGEX → priority-desc first match (invalid
    /// patterns skipped); CSV → JOIN lookup; anything else → MissingConfig.
    fn resolve_role(&self, guild_id: &str, email: &str, mode: &str) -> Result<String, VerifyError> {
        match mode {
            "REGEX" => {
                let rules = self.store.list_regex_rules(guild_id)?;
                for rule in rules {
                    let matched = match regex::Regex::new(&rule.pattern) {
                        Ok(re) => re.is_match(email),
                        Err(e) => {
                            tracing::warn!(error = %e, rule_id = rule.id, "invalid regex pattern, skipping");
                            false
                        }
                    };
                    if !matched {
                        continue;
                    }
                    return Ok(rule.role_id);
                }
                Err(VerifyError::NotActive)
            }
            "CSV" => match self.store.get_role_by_csv_email(guild_id, email)? {
                Some(role_id) => Ok(role_id),
                None => Err(VerifyError::NotActive),
            },
            _ => Err(VerifyError::MissingConfig),
        }
    }
}

/// Parity: Go `generateCode` — CSPRNG value in [0, 1_000_000), zero-padded to 6.
fn generate_code() -> String {
    let n: u32 = rand::rng().random_range(0..1_000_000);
    format!("{n:06}")
}

/// Parity: Go `normalizeCode` — strip all spaces.
fn normalize_code(code: &str) -> String {
    code.chars().filter(|c| *c != ' ').collect()
}

/// Parity: Go `hash` — lowercase hex SHA-256.
fn hash(code: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(code.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Parity: Go `subtle.ConstantTimeCompare`.
fn constant_time_eq(a: &str, b: &str) -> bool {
    use subtle::ConstantTimeEq;
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::GuildConfig;
    use rusqlite::Connection;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI64, Ordering};

    // ---------------------------------------------------------------------
    // Test doubles
    // ---------------------------------------------------------------------

    #[derive(Clone)]
    struct MockMailer {
        sent: Arc<Mutex<Vec<SentRecord>>>,
        fail: bool,
    }

    type SentRecord = (String, String, String, Duration, Locale);

    impl MockMailer {
        fn new() -> Self {
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                fail: true,
            }
        }

        fn sent_count(&self) -> usize {
            self.sent.lock().unwrap().len()
        }

        fn last_send(&self) -> SentRecord {
            self.sent.lock().unwrap().last().cloned().unwrap()
        }
    }

    impl Mailer for MockMailer {
        async fn send_code(
            &self,
            to: &str,
            subject: &str,
            code: &str,
            ttl: Duration,
            locale: Locale,
        ) -> Result<(), MailerError> {
            if self.fail {
                return Err(MailerError::Timeout);
            }
            self.sent.lock().unwrap().push((
                to.to_string(),
                subject.to_string(),
                code.to_string(),
                ttl,
                locale,
            ));
            Ok(())
        }
    }

    fn test_store() -> Store {
        let conn = Connection::open_in_memory().expect("in-memory db");
        let store = Store::from_connection(conn);
        store.migrate().expect("migrate");
        store
    }

    fn seed_guild(store: &Store, guild_id: &str, mode: &str) {
        seed_guild_with_default(store, guild_id, mode, "");
    }

    fn seed_guild_with_default(store: &Store, guild_id: &str, mode: &str, default_role: &str) {
        store
            .save_guild_config(&GuildConfig {
                guild_id: guild_id.into(),
                verify_channel_id: String::new(),
                domain: "school.cz".into(),
                mode: mode.into(),
                subject: "Verification code".into(),
                code_ttl_ns: 600_000_000_000,
                max_attempts: 5,
                rate_limit_count: 3,
                rate_limit_window_ns: 900_000_000_000,
                default_role_id: default_role.into(),
            })
            .expect("seed guild");
    }

    /// Service with a fixed clock.
    fn svc(store: Store, mailer: MockMailer, now: i64) -> VerifyService<MockMailer> {
        VerifyService::with_clock(store, mailer, move || now)
    }

    /// Service with an adjustable clock.
    fn svc_clock(
        store: Store,
        mailer: MockMailer,
        clock: Arc<AtomicI64>,
    ) -> VerifyService<MockMailer> {
        let c = clock.clone();
        VerifyService::with_clock(store, mailer, move || c.load(Ordering::SeqCst))
    }

    fn seed_regex_rule(store: &Store, pattern: &str, role: &str, priority: i64) {
        store
            .add_regex_rule("g", pattern, role, priority)
            .expect("add rule");
    }

    // ---------------------------------------------------------------------
    // Start
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn start_normalizes_email() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        let mailer = MockMailer::new();
        let svc = svc(store.clone(), mailer.clone(), 1_000);

        svc.start("g", "u", "  Foo@School.CZ  ", Locale::En)
            .await
            .expect("start ok");

        let (to, subject, code, ttl, _) = mailer.last_send();
        assert_eq!(to, "foo@school.cz");
        assert_eq!(subject, "Verification code");
        assert_eq!(code.len(), 6);
        assert_eq!(ttl, Duration::from_nanos(600_000_000_000));

        let pending = store.get_pending("g", "u").unwrap().unwrap();
        assert_ne!(pending.code_hash, code, "hash must not be plaintext");
        assert_eq!(pending.expires_at, 1_000 + 600);
        assert_eq!(pending.email, "foo@school.cz");
        assert_eq!(store.count_sends_since("g", "u", 0).unwrap(), 1);
    }

    #[tokio::test]
    async fn start_rejects_invalid_domains() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*", "r1", 0);
        let mailer = MockMailer::new();
        let svc = svc(store, mailer.clone(), 1_000);

        for email in ["user@evil.com", "no-at", "a@b@c", "user@"] {
            assert!(
                matches!(
                    svc.start("g", "u", email, Locale::En).await,
                    Err(VerifyError::InvalidDomain)
                ),
                "email {email:?} must be invalid"
            );
        }
        assert_eq!(mailer.sent_count(), 0);
    }

    #[tokio::test]
    async fn start_requires_configured_guild() {
        let store = test_store();
        let mailer = MockMailer::new();
        let svc = svc(store, mailer, 1_000);
        assert!(matches!(
            svc.start("g", "u", "a@school.cz", Locale::En).await,
            Err(VerifyError::MissingConfig)
        ));
    }

    #[tokio::test]
    async fn start_missing_domain_is_missing_config() {
        let store = test_store();
        store
            .save_guild_config(&GuildConfig {
                guild_id: "g".into(),
                verify_channel_id: String::new(),
                domain: String::new(), // not set
                mode: "REGEX".into(),
                subject: String::new(),
                code_ttl_ns: 600_000_000_000,
                max_attempts: 5,
                rate_limit_count: 3,
                rate_limit_window_ns: 900_000_000_000,
                default_role_id: String::new(),
            })
            .unwrap();
        let svc = svc(store, MockMailer::new(), 1_000);
        assert!(matches!(
            svc.start("g", "u", "a@school.cz", Locale::En).await,
            Err(VerifyError::MissingConfig)
        ));
    }

    #[tokio::test]
    async fn start_rejects_inactive_without_sending() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX"); // no rules → nothing matches
        let mailer = MockMailer::new();
        let svc = svc(store, mailer.clone(), 1_000);

        assert!(matches!(
            svc.start("g", "u", "a@school.cz", Locale::En).await,
            Err(VerifyError::NotActive)
        ));
        assert_eq!(mailer.sent_count(), 0, "no send on inactive email");
    }

    #[tokio::test]
    async fn start_rate_limits_per_window() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        store
            .save_guild_config(&GuildConfig {
                rate_limit_count: 1,
                ..store.get_guild_config("g").unwrap().unwrap()
            })
            .unwrap();
        let mailer = MockMailer::new();
        let clock = Arc::new(AtomicI64::new(1_000));
        let svc = svc_clock(store.clone(), mailer.clone(), clock.clone());

        svc.start("g", "u", "a@school.cz", Locale::En)
            .await
            .expect("first ok");
        assert!(matches!(
            svc.start("g", "u", "b@school.cz", Locale::En).await,
            Err(VerifyError::RateLimited)
        ));

        // Window (15 min = 900s) rolls over.
        clock.store(1_000 + 901, Ordering::SeqCst);
        svc.start("g", "u", "c@school.cz", Locale::En)
            .await
            .expect("allowed after window");
        assert_eq!(mailer.sent_count(), 2);
    }

    #[tokio::test]
    async fn start_rejects_email_bound_to_other_user() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        store
            .set_verified("g", "other", "a@school.cz", "r1")
            .unwrap();
        let service = svc(store, MockMailer::new(), 1_000);

        assert!(matches!(
            service.start("g", "u", "a@school.cz", Locale::En).await,
            Err(VerifyError::EmailAlreadyUsed)
        ));

        // Same user re-verifies fine.
        let store2 = test_store();
        seed_guild(&store2, "g", "REGEX");
        seed_regex_rule(&store2, ".*@school.cz", "r1", 0);
        store2.set_verified("g", "u", "a@school.cz", "r1").unwrap();
        let service = svc(store2, MockMailer::new(), 1_000);
        service
            .start("g", "u", "a@school.cz", Locale::En)
            .await
            .expect("same user ok");
    }

    #[tokio::test]
    async fn start_send_failure_leaves_pending_and_no_log() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        let svc = svc(store.clone(), MockMailer::failing(), 1_000);

        assert!(matches!(
            svc.start("g", "u", "a@school.cz", Locale::En).await,
            Err(VerifyError::SendFailed)
        ));
        // Go parity: pending row persists even though the send failed.
        assert!(store.get_pending("g", "u").unwrap().is_some());
        assert_eq!(store.count_sends_since("g", "u", 0).unwrap(), 0);
    }

    // ---------------------------------------------------------------------
    // Confirm
    // ---------------------------------------------------------------------

    fn seed_pending(store: &Store, code: &str, expires_at: i64, attempts: i64) {
        store
            .upsert_pending(&PendingCode {
                guild_id: "g".into(),
                discord_id: "u".into(),
                email: "a@school.cz".into(),
                code_hash: hash(code),
                expires_at,
                attempts,
            })
            .unwrap();
    }

    #[tokio::test]
    async fn confirm_no_pending() {
        let store = test_store();
        let svc = svc(store, MockMailer::new(), 1_000);
        assert!(matches!(
            svc.confirm("g", "u", "123456").await,
            Err(VerifyError::NoPending)
        ));
    }

    #[tokio::test]
    async fn confirm_expired_deletes_pending() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        seed_pending(&store, "123456", 900, 0); // expires before now=1000
        let svc = svc(store.clone(), MockMailer::new(), 1_000);

        assert!(matches!(
            svc.confirm("g", "u", "123456").await,
            Err(VerifyError::Expired)
        ));
        assert!(store.get_pending("g", "u").unwrap().is_none());
    }

    #[tokio::test]
    async fn confirm_wrong_code_tracks_attempts() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let svc = svc(store.clone(), MockMailer::new(), 1_000);

        // max_attempts = 5; each wrong code reports remaining after increment.
        for expected_remaining in (1..=4).rev() {
            match svc.confirm("g", "u", "000000").await {
                Err(VerifyError::WrongCode { remaining }) => {
                    assert_eq!(remaining, expected_remaining);
                }
                other => panic!("expected WrongCode, got {other:?}"),
            }
        }
        // 5th wrong code → too many attempts, pending deleted.
        assert!(matches!(
            svc.confirm("g", "u", "000000").await,
            Err(VerifyError::TooManyAttempts)
        ));
        assert!(store.get_pending("g", "u").unwrap().is_none());
    }

    #[tokio::test]
    async fn confirm_strips_spaces_in_code() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let svc = svc(store, MockMailer::new(), 1_000);

        let roles = svc
            .confirm("g", "u", " 1 2 3 4 5 6 ")
            .await
            .expect("spaces stripped");
        assert_eq!(roles, vec!["r1"]);
    }

    #[tokio::test]
    async fn confirm_success_assigns_primary_and_saves_verified() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let svc = svc(store.clone(), MockMailer::new(), 1_000);

        let roles = svc.confirm("g", "u", "123456").await.expect("confirm ok");
        assert_eq!(roles, vec!["r1"]);

        let v = store
            .get_verified_by_email("g", "a@school.cz")
            .unwrap()
            .unwrap();
        assert_eq!(v.discord_id, "u");
        assert_eq!(v.role_id, "r1", "stored role is the primary");
        assert!(store.get_pending("g", "u").unwrap().is_none());
    }

    #[tokio::test]
    async fn confirm_appends_default_role_when_different() {
        let store = test_store();
        seed_guild_with_default(&store, "g", "REGEX", "rDefault");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let svc = svc(store.clone(), MockMailer::new(), 1_000);

        let roles = svc.confirm("g", "u", "123456").await.expect("confirm ok");
        assert_eq!(roles, vec!["r1", "rDefault"]);
        assert_eq!(
            store
                .get_verified_by_email("g", "a@school.cz")
                .unwrap()
                .unwrap()
                .role_id,
            "r1"
        );
    }

    #[tokio::test]
    async fn confirm_default_role_skipped_when_same_or_empty() {
        // Same as primary.
        let store = test_store();
        seed_guild_with_default(&store, "g", "REGEX", "r1");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let roles = svc(store, MockMailer::new(), 1_000)
            .confirm("g", "u", "123456")
            .await
            .expect("confirm ok");
        assert_eq!(roles, vec!["r1"]);

        // No default configured.
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        seed_regex_rule(&store, ".*@school.cz", "r1", 0);
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let roles = svc(store, MockMailer::new(), 1_000)
            .confirm("g", "u", "123456")
            .await
            .expect("confirm ok");
        assert_eq!(roles, vec!["r1"]);
    }

    #[tokio::test]
    async fn confirm_resolve_failure_deletes_pending() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX"); // no rules → resolve fails
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let svc = svc(store.clone(), MockMailer::new(), 1_000);

        assert!(matches!(
            svc.confirm("g", "u", "123456").await,
            Err(VerifyError::NotActive)
        ));
        assert!(store.get_pending("g", "u").unwrap().is_none());
    }

    // ---------------------------------------------------------------------
    // Role resolution
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn regex_priority_order_and_invalid_pattern_skip() {
        let store = test_store();
        seed_guild(&store, "g", "REGEX");
        // Invalid pattern must be skipped even though it has the highest priority.
        store.add_regex_rule("g", "[", "bogus", 50).unwrap();
        store
            .add_regex_rule("g", ".*@admin.cz", "admin", 40)
            .unwrap();
        store
            .add_regex_rule("g", ".*@school.cz", "student", 10)
            .unwrap();
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let svc = svc(store, MockMailer::new(), 1_000);

        let roles = svc.confirm("g", "u", "123456").await.expect("confirm ok");
        assert_eq!(
            roles,
            vec!["student"],
            "student email matched; invalid rule skipped"
        );
    }

    #[tokio::test]
    async fn csv_role_resolution() {
        let store = test_store();
        seed_guild(&store, "g", "CSV");
        store.insert_csv_email("g", "a@school.cz", "3A").unwrap();
        store.map_csv_class("g", "3A", "roleA").unwrap();
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let svc = svc(store, MockMailer::new(), 1_000);

        let roles = svc.confirm("g", "u", "123456").await.expect("confirm ok");
        assert_eq!(roles, vec!["roleA"]);
    }

    #[tokio::test]
    async fn csv_unknown_email_not_active() {
        let store = test_store();
        seed_guild(&store, "g", "CSV");
        seed_pending(&store, "123456", 1_000 + 600, 0);
        let svc = svc(store, MockMailer::new(), 1_000);
        assert!(matches!(
            svc.confirm("g", "u", "123456").await,
            Err(VerifyError::NotActive)
        ));
    }

    // ---------------------------------------------------------------------
    // Helpers + localization
    // ---------------------------------------------------------------------

    #[test]
    fn generate_code_is_six_zero_padded_digits() {
        for _ in 0..500 {
            let code = generate_code();
            assert_eq!(code.len(), 6, "code {code:?}");
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn hash_is_sha256_hex() {
        assert_eq!(hash("123456").len(), 64);
        assert_eq!(
            hash("123456"),
            "8d969eef6ecad3c29a3a629280e686cf0c3f5d5a86aff3ca12020c923adc6c92"
        );
    }

    #[test]
    fn normalize_code_strips_all_spaces() {
        assert_eq!(normalize_code(" 1 2 3 4 5 6 "), "123456");
        assert_eq!(normalize_code("123456"), "123456");
    }

    #[test]
    fn localize_maps_all_user_facing_errors() {
        let t = crate::i18n::get(Locale::En);
        assert_eq!(VerifyError::MissingConfig.localize(t), t.err_missing_config);
        assert_eq!(VerifyError::InvalidDomain.localize(t), t.err_invalid_domain);
        assert_eq!(VerifyError::NotActive.localize(t), t.err_not_active);
        assert_eq!(
            VerifyError::EmailAlreadyUsed.localize(t),
            t.err_email_already_used
        );
        assert_eq!(VerifyError::RateLimited.localize(t), t.err_rate_limited);
        assert_eq!(VerifyError::NoPending.localize(t), t.err_no_pending);
        assert_eq!(VerifyError::Expired.localize(t), t.err_expired);
        assert_eq!(
            VerifyError::TooManyAttempts.localize(t),
            t.err_too_many_attempts
        );
        assert_eq!(VerifyError::SendFailed.localize(t), t.err_send_failed);
        assert_eq!(
            VerifyError::WrongCode { remaining: 3 }.localize(t),
            "Wrong code. 3 attempts remaining."
        );
    }
}
