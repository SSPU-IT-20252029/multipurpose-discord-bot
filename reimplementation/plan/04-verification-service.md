# Session 4 — Verification Service

> **Goal:** `verify.rs` — the entire email verification business logic as a pure, testable
> service. This is the heart of the bot.
>
> **Parity target:** Go `internal/verify/service.go` (218 lines) + the `resolveRole` logic
> described in `reimplementation/03-role-resolution.md` + `02-email-verification-flow.md`.

---

## 4.1 Service struct (implemented)

```rust
// Parity: Go `verify.Mailer` interface → crate-internal async trait, so the
// service is GENERIC over `M` (RPITIT is not object-safe).
#[allow(async_fn_in_trait)]
pub trait Mailer: Send + Sync {
    async fn send_code(&self, to: &str, subject: &str, code: &str,
                       ttl: Duration, locale: Locale) -> Result<(), MailerError>;
}
// impl Mailer for crate::mailer::Mailer (lives in verify.rs)

pub struct VerifyService<M: Mailer> {
    store: Store,
    mailer: M,
    now: Arc<dyn Fn() -> i64 + Send + Sync>,   // injectable clock, Go `Now func()`
}
```

- `new(store, mailer)` uses real time; `with_clock(store, mailer, now)` injects a clock (tests
  drive it with an `AtomicI64`).
- `start`/`confirm` return the domain `VerifyError` (below), which carries a
  `localize(&self, t) -> String` port of Go `localizeError`.

> **Corrected parity facts (verified against `internal/verify/service.go`):**
> - `Confirm(ctx, guildID, discordID, code)` returns **`[]string`** (role IDs) and takes **no
>   locale** — handlers assign roles and reply.
> - `Start` **normalizes the email first**: `strings.ToLower(strings.TrimSpace(email))`.
> - The email subject sent is **`cfg.Subject` verbatim**; the localized `DefaultSubject` default
>   is applied in the `/setup` command layer, not the service.
> - Rate-limit cutoff is `now - RateLimitWindow` where the window is **nanoseconds** in the DB →
>   convert with `window_ns / 1_000_000_000` at the service boundary.
> - On `SendCode` failure the **pending row remains** (Go stores it before sending).
> - A `resolveRole` failure inside `Confirm` **deletes the pending code** before returning.

## 4.2 Error taxonomy (implemented)

```rust
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    MissingConfig, InvalidDomain, NotActive, EmailAlreadyUsed, RateLimited,
    NoPending, Expired, TooManyAttempts,
    WrongCode { remaining: i64 },
    SendFailed,
    Store(#[from] StoreError),   // internal; localize → raw error string (Go err.Error() parity)
}
```
    #[error("wrong code, {remaining} attempt(s) left")]
    WrongCode { remaining: u8 },
    #[error("failed to send email")]
    SendFailed,
}
```

> Go also had `ErrSendFailed` and `WrongCodeError{Remaining}` — mapped above. Internal
> plumbing errors (`StoreError`, `MailerError`) are stored separately in `StartOutcome` /
> `ConfirmOutcome` so the UI can distinguish "user recoverable" vs "server broke".

## 4.3 Role resolution (implemented)

```rust
// Returns the PRIMARY role id only. The default-role merge happens in `confirm`
// (Go parity): roles = [primary] (+ default if set && different).
fn resolve_role(&self, guild_id: &str, email: &str, mode: &str) -> Result<String, VerifyError>
```

- **REGEX**: rules ordered `priority DESC`; first `re.is_match(email)` wins; **invalid patterns are
  skipped** (Go `err || !matched → continue`, logged as a warning). No match → `NotActive`.
- **CSV**: `get_role_by_csv_email` JOIN; no row → `NotActive`.
- Any other mode value → `MissingConfig`.

## 4.4 Code generation + hashing (implemented)

```rust
fn generate_code() -> String {                    // Go crypto/rand, %06d
    let n: u32 = rand::rng().random_range(0..1_000_000);
    format!("{n:06}")
}
fn normalize_code(code: &str) -> String { code.chars().filter(|c| *c != ' ').collect() } // strip ALL spaces
fn hash(code: &str) -> String { /* SHA-256 lowercase hex */ }
fn constant_time_eq(a: &str, b: &str) -> bool { /* subtle::ConstantTimeEq */ }
```

## 4.5 Start flow (implemented)

```rust
pub async fn start(&self, guild_id: &str, discord_id: &str, email: &str,
                   locale: Locale) -> Result<(), VerifyError>
```

1. `email = email.trim().to_ascii_lowercase()` (**normalize first** — Go parity).
2. GuildConfig must exist **and** have a non-empty `domain` → else `MissingConfig`.
3. Domain strict-match: `split('@')` must yield **exactly 2 parts** and `parts[1] == cfg.domain`
   (case-sensitive) → else `InvalidDomain`.
4. Pre-resolve role (rejects inactive emails before sending).
5. `now = self.now()`.
6. Squatting: `get_verified_by_email` where `discord_id != caller` → `EmailAlreadyUsed`.
7. Rate limit: `count_sends_since(guild, user, now - window_ns/1e9) >= rate_limit_count` →
   `RateLimited`.
8. `generate_code` → `upsert_pending` (SHA-256 hash, `expires = now + ttl_ns/1e9`, attempts 0).
9. `mailer.send_code(email, cfg.subject, code, Duration::from_nanos(ttl_ns), locale)`; on error
   **log the cause** and return `SendFailed` (**pending row remains** — Go stores before sending).
10. `log_send(guild, user, now)` (includes the 2h prune).

> Go logging confirmed: `UpsertPending` → `SendCode` → `LogSend`, in that order. No separate
> prune call (it lives inside `LogSend`). `cfg.subject` is sent verbatim; the `/setup` layer
> defaults it to `DefaultSubject`.

## 4.6 Confirm flow (implemented)

```rust
pub async fn confirm(&self, guild_id: &str, discord_id: &str, code: &str)
    -> Result<Vec<String>, VerifyError>     // role IDs, [primary] (+ default)
```

1. `get_pending` → else `NoPending`.
2. GuildConfig must exist → else `MissingConfig`.
3. `now > expires_at` → delete pending + `Expired`.
4. Constant-time compare `hash(normalize_code(code))` vs stored hash:
   - mismatch → `increment_attempts`; `attempts >= max_attempts` → delete pending +
     `TooManyAttempts`; else `WrongCode { remaining: max - attempts }`.
5. Re-resolve role (fresh); **any error deletes the pending code** then returns.
6. `set_verified(guild, user, pending.email, primary_role)` → delete pending.
7. Return `[primary]` (+ `default_role_id` when set and different).

## 4.7 Helpers (implemented)

`hash` (sha2 hex), `constant_time_eq` (subtle), `now_unix_secs` (SystemTime). `StoreError`
converts into `VerifyError::Store` via `#[from]` for `?`.

## 4.8 What handlers receive

`start` → `Result<(), VerifyError>`; `confirm` → `Result<Vec<String>, VerifyError>` (role IDs).
Handlers (Sessions 6–7) call `err.localize(t)` and, for `start`, wrap the result in
`t.err_email_fmt` ("Error: %s") — exact Go `handleModal` behavior. Role assignment loops the
returned list with `add_member_role`.

---

## 4.9 Unit tests

Test with in-memory store (`Store::from_connection` + `migrate`) and a `MockMailer` (counting
`MockMailer` implementing the `Mailer` trait), plus an injected clock (`AtomicI64`).

| Area | Test |
|---|---|
| normalize | `"  Foo@School.CZ "` → stored/sent as `foo@school.cz` |
| domain | `user@evil.com`, no `@`, `a@b@c`, `user@` → `InvalidDomain`; no mailer calls |
| config | missing guild / empty domain → `MissingConfig` |
| role pre-check | regex no-match → `NotActive`, **0 mailer calls** |
| rate limit | `count` sends allowed; `count+1` → `RateLimited`; window rollover (clock advance) allows |
| squatting | email verified by other user → `EmailAlreadyUsed`; same user re-verifies OK |
| start success | pending hashed (not plaintext), expires = now + ttl, send_log = 1, subject/ttl passed |
| code format | `generate_code` 6 zero-padded digits over 500 iterations |
| send failure | → `SendFailed`, **pending remains**, no send_log |
| confirm | no pending → `NoPending` |
| confirm | expired → deletes pending + `Expired` |
| confirm | wrong code ×4 → `WrongCode{4..1}`; 5th → `TooManyAttempts` + pending deleted |
| confirm | `" 1 2 3 4 5 6 "` matches hash of `"123456"` |
| confirm success | saves verified (stored role = primary), deletes pending, returns `[primary]` |
| confirm default role | returns `[primary, default]` when different; `[primary]` when same or unset |
| regex parity | higher priority wins; invalid pattern skipped |
| csv | role via JOIN; unknown email → `NotActive` |
| confirm resolve failure | pending deleted + error returned |
| localize | every user-facing variant maps to the EN translation; `WrongCode{3}` → "Wrong code. 3 attempts remaining." |

---

## 4.10 Session completion checklist

- [ ] `VerifyService::start` / `confirm` implemented with exact Go validation order
- [ ] `resolve_role` REGEX + CSV + default-role merge
- [ ] `generate_code` CSPRNG, zero-padded 6 digits
- [ ] SHA256 storage + constant-time compare
- [ ] Full error taxonomy, outcomes for handlers
- [ ] All unit tests green (incl. mailer-mock assertion of "no send on NotActive")
- [ ] clippy/fmt clean
