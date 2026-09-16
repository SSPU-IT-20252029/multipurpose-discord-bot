# Session 3 — i18n System + Mailer

> **Goal:** `i18n.rs` — bilingual (EN/CS) static translation system mirroring the Go `Translations`
> struct (~140 fields); and `mailer.rs` — a Resend API client with HTML + plain-text templates.
>
> **Parity target:** Go `internal/i18n/i18n.go` (403 lines) and `internal/mailer/mailer.go`.

---

## 3.1 Locale type & parsing

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Locale { #[default] En, Cs }

impl Locale {
    /// Accepts: cs, cz, cs-cz, cs_CZ, czech (case-insensitive) → Cs; everything else → En.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "cs" | "cz" | "cs-cz" | "cs_cz" | "czech" => Locale::Cs,
            _ => Locale::En,
        }
    }

    pub fn code(self) -> &'static str { match self { Locale::En => "en", Locale::Cs => "cs" } }
}
```

## 3.2 Translations struct (implemented)

**114 fields**, ported exhaustively from `internal/i18n/i18n.go` (verified count). Every field is
a `&'static str`; field names are snake_case of the Go field names (no `_desc` suffixes). Both
locale constants (`static EN`, `static CS`) fill **every** field — the compiler enforces
completeness (an advantage over Go).

Go format verbs are converted: `%s`/`%d` → `{}`. Format application uses the [`subst`] port of
`fmt.Sprintf` (replaces `{}` sequentially; trailing unused placeholders are preserved). Up to 2
placeholders per string (`class_mapped_fmt`). Field categories:

- **Command descriptions/options**: `setup_desc`, `setup_domain`, `setup_mode`, `setup_channel`,
  `setup_subject`, `regex_desc`, `regex_add/list/remove`, `regex_pattern/role/priority/id`,
  `csv_desc`, `csv_upload/map/file/class/role`, `language_desc`, `language_set_fmt`,
  `rate_limit_desc/count_desc/window_desc/set_fmt`, `verified_role_desc/set/view/clear/role/
  set_fmt/view_fmt/cleared/not_set`, `help_desc/text/admin_title/user_title/click_hint`
- **Verify flow**: `verify_btn`, `enter_code_btn`, `verify_modal_title`, `your_email`,
  `email_placeholder`, `code_modal_title`, `code_label`, `code_placeholder`, `embed_title`,
  `embed_desc_fmt`, `code_sent_fmt`, `verify_success`
- **CRUD feedback**: `config_saved`, `config_saved_err`, `rule_added`, `no_rules`,
  `rule_deleted`, `failed_save/load_rules/delete/map`, `uploaded_emails_fmt`, `invalid_csv`,
  `error_download`, `class_mapped_fmt`
- **Verification errors**: `err_not_active/rate_limited/no_pending/expired/too_many_attempts/
  send_failed/email_already_used/invalid_domain/missing_config/wrong_code_fmt/email_fmt/send_fmt`
- **Email template**: `email_hello/code_for/copy_btn/valid_for_fmt/sender_fallback`,
  `default_subject`, `html_title`
- **Backup**: `backup_desc/create/restore/list/schedule/schedule_off/delete/scope/scope_single/
  scope_multi/guild_id/freq/time_of_day/created_fmt/restored_fmt/no_backups/deleted_fmt/
  scheduled_fmt/scheduled_off/schedule_disabled/error_capture/error_restore/error_save/error_list/
  error_delete/confirm_restore/confirm_delete/next_run_fmt/manual_label/scheduled_label/all_label`

## 3.3 Locale lookup

```rust
pub fn get(locale: Locale) -> Translations {
    match locale {
        Locale::En => TRANSLATIONS_EN,
        Locale::Cs => TRANSLATIONS_CS,
    }
}

static TRANSLATIONS_EN: Translations = Translations { /* …140 fields… */ };
static TRANSLATIONS_CS: Translations = Translations { /* …same fields, Czech… */ };
```

`static` (not `lazy_static`/`OnceLock`) because all values are `&'static str` — zero-cost.

## 3.4 Helpers used by handlers

```rust
/// Resolve a guild member's stored locale, falling back to English.
pub fn locale_for(store: &Store, guild_id: &str, user_id: &str) -> Locale {
    store.get_user_locale(guild_id, user_id)
        .ok().flatten()
        .map(|l| Locale::parse(&l))
        .unwrap_or(Locale::En)
}
```

Session 5 introduces a `Bot::get_locale(ctx)` wrapper that re-fetches the locale during modal
flow (the user may have changed `/language` mid-verification — Go re-fetches in `handleModal`).

## 3.5 Mailer — Resend client

```rust
pub struct Mailer {
    http: reqwest::Client,
    api_key: String,
    from: String,
    send_timeout: std::time::Duration,   // Go uses 30s
}

impl Mailer {
    pub fn new(api_key: String, from: String) -> Self { … }

    /// Send a verification code email. `t` = locale translations.
    pub async fn send_code(
        &self,
        to: &str,
        code: &str,
        subject: &str,
        ttl: std::time::Duration,
        t: Translations,
    ) -> Result<(), MailerError> {
        let (plain, html) = render_email(code, ttl, t);
        let payload = serde_json::json!({
            "from": self.from,
            "to": [to],
            "subject": subject,
            "text": plain,
            "html": html,
        });
        self.http
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .timeout(self.send_timeout)
            .send()
            .await?;
        // Treat non-2xx as failure (Go's resend-go returns typed API errors).
        Ok(())
    }
}
```

## 3.6 Email templates (implemented — ported 1:1 from Go)

**Plain text** (`build_text`) — lines joined with `\r\n`: `EmailHello`, empty, `EmailCodeFor`,
empty, `"    " + code` (4-space indent), empty, `"[" + EmailCopyBtn + "]"`, empty,
`EmailValidForFmt` with `minutes = ttl.as_secs() / 60`, empty, `senderName`.

**HTML** (`build_html`) — the exact Go template kept as a `const HTML_TEMPLATE` with the same
tokens `{{.Code}} {{.Hello}} {{.CodeFor}} {{.CopyBtn}} {{.ValidFor}} {{.Sender}} {{.Lang}}
{{.Title}}`; rendered via sequential `.replace()` (no template engine needed). On any token being
absent from the const, the test suite fails (no `{{` may remain).

**`sender_name(from, locale)`** — substring before the first `<` (trimmed), else
`EmailSenderFallback`.

**TTL minutes** — `ttl.as_secs() / 60`, parity with Go `int(ttl.Minutes())` truncation.

## 3.7 Mailer error

```rust
#[derive(Debug, thiserror::Error)]
pub enum MailerError {
    #[error("resend api error (status {status}): {body}")]
    Api { status: u16, body: String },
    #[error("http error: {0}")] Http(#[from] reqwest::Error),
    #[error("timeout sending email")] Timeout,
}
```

Map `reqwest::Error::is_timeout` to `Timeout`. Non-2xx responses become `Api { status, body }`.

**Implemented constructor surface** (deviation for testability):
- `Mailer::new(api_key, from)` — prod endpoint `https://api.resend.com/emails`, 30s timeout.
- `Mailer::with_base(base_url, api_key, from)` — tests point at an `httpmock` server.
- `Mailer::send_timeout(duration)` — builder override so the timeout test runs fast.

---

## 3.8 Unit tests

| Module | Test |
|---|---|
| `i18n` | `parse` maps `cs`/`cz`/`cs-cz`/`cs_CZ`/`czech` (+ whitespace/case variants) → `Cs` |
| `i18n` | `parse` maps everything else → `En` (incl. empty, `en`, `de`, `XX`, `csx`) |
| `i18n` | `get(En)` / `get(Cs)`: every one of the 114 fields non-empty |
| `i18n` | `subst` — 1 arg, 2 args (`class_mapped_fmt`), no args (placeholder preserved), arg containing `{}` |
| `mailer` | `httpmock` 200 → Ok; matchers verify `authorization` header (case-insensitive), `json_body_partial` (from/to/subject), `body_contains` (code + html title) |
| `mailer` | mock returns 429 → `MailerError::Api { status: 429 }` |
| `mailer` | mock delay > timeout (300ms override) → `MailerError::Timeout` |
| `mailer` | `build_text` exact CRLF structure (4-space code indent, `[COPY]`, 10 min, sender) |
| `mailer` | `build_html` replaces all tokens (no `{{` remains), includes code/lang/title; CS variant too |
| `mailer` | `sender_name` — with `<` returns trimmed name; without → localized fallback |

> `httpmock 0.7` removed the request-retrieval API; assertions use `when.matches(fn)` (non-capturing
> closure over `HttpMockRequest`), `json_body_partial`, and `body_contains` instead.

---

## 3.9 Session completion checklist

- [ ] `Locale::parse` complete with all Czech aliases
- [ ] `Translations` struct has **every** field from Go `i18n.go` (both locales filled)
- [ ] `get(locale)` returns EN/CS statics
- [ ] `Mailer::send_code` with 30s timeout, auth header, JSON payload
- [ ] Email templates ported 1:1
- [ ] `i18n` + `mailer` tests green
- [ ] clippy/fmt clean
