//! Resend API email sender.
//!
//! Parity target: Go `internal/mailer/mailer.go`. Sends verification codes as
//! JSON to `POST https://api.resend.com/emails` with a 30s timeout.

use crate::i18n::{self, Locale};
use serde::Serialize;
use std::time::Duration;

const SEND_TIMEOUT: Duration = Duration::from_secs(30);
const RESEND_URL: &str = "https://api.resend.com/emails";

#[derive(Debug, thiserror::Error)]
pub enum MailerError {
    #[error("resend api error (status {status}): {body}")]
    Api { status: u16, body: String },
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("timeout sending email")]
    Timeout,
}

#[derive(Clone)]
pub struct Mailer {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    from: String,
    send_timeout: Duration,
}

#[derive(Serialize)]
struct ResendPayload {
    from: String,
    to: Vec<String>,
    subject: String,
    text: String,
    html: String,
}

impl Mailer {
    pub fn new(api_key: String, from: String) -> Self {
        Self::with_base(RESEND_URL.to_string(), api_key, from)
    }

    /// Constructor for tests pointing at an `httpmock` server.
    pub fn with_base(base_url: String, api_key: String, from: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            api_key,
            from,
            send_timeout: SEND_TIMEOUT,
        }
    }

    /// Override the request timeout (used by the timeout test).
    pub fn send_timeout(mut self, timeout: Duration) -> Self {
        self.send_timeout = timeout;
        self
    }

    /// Parity: Go `Mailer.SendCode(to, subject, code, ttl, locale)`.
    pub async fn send_code(
        &self,
        to: &str,
        subject: &str,
        code: &str,
        ttl: Duration,
        locale: Locale,
    ) -> Result<(), MailerError> {
        let payload = ResendPayload {
            from: self.from.clone(),
            to: vec![to.to_string()],
            subject: subject.to_string(),
            text: self.build_text(code, ttl, locale),
            html: self.build_html(code, ttl, locale),
        };

        let res = self
            .http
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .timeout(self.send_timeout)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    MailerError::Timeout
                } else {
                    MailerError::Http(e)
                }
            })?;

        let status = res.status();
        if !status.is_success() {
            let body = res.text().await.unwrap_or_default();
            return Err(MailerError::Api {
                status: status.as_u16(),
                body,
            });
        }
        Ok(())
    }

    /// Parity: Go `buildText` — CRLF-joined lines with a 4-space-indented code.
    fn build_text(&self, code: &str, ttl: Duration, locale: Locale) -> String {
        let t = i18n::get(locale);
        let minutes = ttl.as_secs() / 60;
        [
            t.email_hello.to_string(),
            String::new(),
            t.email_code_for.to_string(),
            String::new(),
            format!("    {code}"),
            String::new(),
            format!("[{}]", t.email_copy_btn),
            String::new(),
            i18n::subst(t.email_valid_for_fmt, &[&minutes.to_string()]),
            String::new(),
            sender_name(&self.from, locale),
        ]
        .join("\r\n")
    }

    /// Parity: Go `buildHTML` — the exact template with `{{.X}}` tokens.
    fn build_html(&self, code: &str, ttl: Duration, locale: Locale) -> String {
        let t = i18n::get(locale);
        let minutes = ttl.as_secs() / 60;
        let valid_for = i18n::subst(t.email_valid_for_fmt, &[&minutes.to_string()]);
        HTML_TEMPLATE
            .replace("{{.Lang}}", locale.code())
            .replace("{{.Title}}", t.html_title)
            .replace("{{.Hello}}", t.email_hello)
            .replace("{{.CodeFor}}", t.email_code_for)
            .replace("{{.Code}}", code)
            .replace("{{.CopyBtn}}", t.email_copy_btn)
            .replace("{{.ValidFor}}", &valid_for)
            .replace("{{.Sender}}", &sender_name(&self.from, locale))
    }
}

/// Parity: Go `senderName` — the display name before `<` in the from address,
/// falling back to the localized sender fallback.
fn sender_name(from: &str, locale: Locale) -> String {
    match from.find('<') {
        Some(pos) if pos > 0 => from[..pos].trim().to_string(),
        _ => i18n::get(locale).email_sender_fallback.to_string(),
    }
}

/// Ported verbatim from Go `mailer.go` (tokens are replaced at render time).
const HTML_TEMPLATE: &str = r#"<!doctype html>
<html lang="{{.Lang}}">
<body style="margin:0;padding:0;background-color:#f4f5f7;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Arial,sans-serif;">
<table role="presentation" width="100%" cellpadding="0" cellspacing="0"><tr><td align="center" style="padding:32px 16px;">
<table role="presentation" width="480" cellpadding="0" cellspacing="0" style="max-width:480px;width:100%;background-color:#ffffff;border-radius:12px;">
<tr><td colspan="3" style="height:32px;"></td></tr>
<tr><td style="width:32px;"></td><td style="font-size:20px;font-weight:700;color:#111827;">{{.Title}}</td><td style="width:32px;"></td></tr>
<tr><td colspan="3" style="height:8px;"></td></tr>
<tr><td style="width:32px;"></td><td style="font-size:15px;line-height:22px;color:#374151;">{{.Hello}}<br>{{.CodeFor}}</td><td style="width:32px;"></td></tr>
<tr><td colspan="3" style="height:24px;"></td></tr>
<tr><td style="width:32px;"></td><td align="center" style="padding:0 0 24px 0;">
<table role="presentation" cellpadding="0" cellspacing="0" style="margin:0 auto;">
<tr>
<td style="background-color:#eef2ff;border:1px solid #c7d2fe;border-radius:10px 0 0 10px;padding:14px 24px;font-family:'SF Mono',Consolas,Menlo,monospace;font-size:34px;font-weight:700;letter-spacing:10px;color:#1d4ed8;user-select:all;-webkit-user-select:all;">{{.Code}}</td>
<td style="background-color:#c7d2fe;border-radius:0 10px 10px 0;padding:14px 18px;font-size:13px;font-weight:700;color:#3730a3;text-transform:uppercase;letter-spacing:1px;white-space:nowrap;">{{.CopyBtn}}</td>
</tr>
</table>
</td><td style="width:32px;"></td></tr>
<tr><td style="width:32px;"></td><td style="font-size:13px;line-height:20px;color:#6b7280;">{{.ValidFor}}</td><td style="width:32px;"></td></tr>
<tr><td colspan="3" style="height:24px;"></td></tr>
<tr><td style="width:32px;"></td><td style="font-size:13px;color:#9ca3af;">{{.Sender}}</td><td style="width:32px;"></td></tr>
<tr><td colspan="3" style="height:32px;"></td></tr>
</table>
</td></tr></table>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;

    const FROM: &str = "Discord bot <discord-bot@example.com>";
    const TTL: Duration = Duration::from_secs(600); // 10 min

    #[tokio::test]
    async fn send_code_success_posts_resend_payload() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/emails")
                .matches(|req| {
                    // Non-capturing fn-pointer matcher: verify Bearer auth (case-insensitive name).
                    req.headers
                        .as_ref()
                        .and_then(|hs| {
                            hs.iter()
                                .find(|(n, _)| n.eq_ignore_ascii_case("authorization"))
                                .map(|(_, v)| v.as_str())
                        })
                        == Some("Bearer re_mykey")
                })
                .json_body_partial(
                    r#"{"from":"Discord bot <discord-bot@example.com>","to":["student@school.cz"],"subject":"Verification code"}"#,
                )
                .body_contains("123456")
                .body_contains("Discord Verification");
            then.status(200).json_body(serde_json::json!({ "id": "email_123" }));
        });

        let mailer = Mailer::with_base(
            server.url("/emails"),
            "re_mykey".to_string(),
            FROM.to_string(),
        );

        mailer
            .send_code(
                "student@school.cz",
                "Verification code",
                "123456",
                TTL,
                Locale::En,
            )
            .await
            .expect("send should succeed");

        mock.assert();
    }

    #[tokio::test]
    async fn send_code_api_error_returns_status() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/emails");
            then.status(429).body("rate limited");
        });

        let mailer = Mailer::with_base(server.url("/emails"), "k".into(), FROM.into());
        let err = mailer
            .send_code("a@b.cz", "s", "000000", TTL, Locale::En)
            .await
            .expect_err("429 must fail");

        match err {
            MailerError::Api { status, body } => {
                assert_eq!(status, 429);
                assert_eq!(body, "rate limited");
            }
            other => panic!("expected Api error, got {other:?}"),
        }
        mock.assert();
    }

    #[tokio::test]
    async fn send_code_timeout() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/emails");
            then.delay(Duration::from_secs(5)).status(200);
        });

        let mailer = Mailer::with_base(server.url("/emails"), "k".into(), FROM.into())
            .send_timeout(Duration::from_millis(300));
        let err = mailer
            .send_code("a@b.cz", "s", "000000", TTL, Locale::En)
            .await
            .expect_err("timeout must fail");
        assert!(matches!(err, MailerError::Timeout), "got {err:?}");
        mock.assert();
    }

    #[test]
    fn build_text_structure() {
        let mailer = Mailer::new("k".into(), FROM.into());
        let text = mailer.build_text("123456", TTL, Locale::En);
        let lines: Vec<&str> = text.split("\r\n").collect();
        assert_eq!(lines[0], "Hello,");
        assert_eq!(lines[1], "");
        assert_eq!(lines[2], "Your verification code for Discord is:");
        assert_eq!(lines[3], "");
        assert_eq!(lines[4], "    123456");
        assert_eq!(lines[5], "");
        assert_eq!(lines[6], "[COPY]");
        assert_eq!(lines[7], "");
        assert_eq!(
            lines[8],
            "The code is valid for 10 minutes. If you did not request this, please ignore this email."
        );
        assert_eq!(lines[9], "");
        assert_eq!(lines[10], "Discord bot");
    }

    #[test]
    fn build_html_replaces_all_tokens() {
        let mailer = Mailer::new("k".into(), FROM.into());
        let html = mailer.build_html("123456", TTL, Locale::En);
        assert!(!html.contains("{{."), "template tokens must be replaced");
        assert!(html.contains("123456"));
        assert!(html.contains("Discord Verification"));
        assert!(html.contains("lang=\"en\""));
        assert!(html.contains("Discord bot"));
    }

    #[test]
    fn build_html_czech_locale() {
        let mailer = Mailer::new("k".into(), FROM.into());
        let html = mailer.build_html("123456", TTL, Locale::Cs);
        assert!(html.contains("lang=\"cs\""));
        assert!(html.contains("Ověření Discordem"));
        assert!(html.contains("ZKOPÍROVAT"));
    }

    #[test]
    fn sender_name_from_display_name() {
        assert_eq!(
            sender_name("Discord bot <x@y.z>", Locale::En),
            "Discord bot"
        );
        assert_eq!(sender_name("   Padded   <x@y.z>", Locale::En), "Padded");
    }

    #[test]
    fn sender_name_falls_back() {
        assert_eq!(sender_name("bare@y.z", Locale::En), "Discord bot");
        assert_eq!(sender_name("bare@y.z", Locale::Cs), "Discord bot");
    }
}
