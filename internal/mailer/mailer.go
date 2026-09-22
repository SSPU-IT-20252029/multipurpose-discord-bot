// Package mailer sends verification codes via email using the Resend API.
package mailer

import (
	"context"
	"fmt"
	"html/template"
	"strings"
	"time"

	"github.com/resend/resend-go/v4"

	"sspu-multipurpose-discord-bot/internal/config"
	"sspu-multipurpose-discord-bot/internal/i18n"
)

const sendTimeout = 30 * time.Second

type Mailer struct {
	client *resend.Client
	cfg    config.Email
}

func New(cfg config.Email) *Mailer {
	return &Mailer{client: resend.NewClient(cfg.APIKey), cfg: cfg}
}

func (m *Mailer) SendCode(to, subject, code string, ttl time.Duration, locale i18n.Locale) error {
	ctx, cancel := context.WithTimeout(context.Background(), sendTimeout)
	defer cancel()

	_, err := m.client.Emails.SendWithContext(ctx, &resend.SendEmailRequest{
		From:    m.cfg.From,
		To:      []string{to},
		Subject: subject,
		Text:    m.buildText(code, ttl, locale),
		Html:    m.buildHTML(code, ttl, locale),
	})
	if err != nil {
		return fmt.Errorf("sending email via Resend: %w", err)
	}
	return nil
}

func (m *Mailer) buildText(code string, ttl time.Duration, locale i18n.Locale) string {
	t := i18n.Get(locale)
	return strings.Join([]string{
		t.EmailHello,
		"",
		t.EmailCodeFor,
		"",
		"    " + code,
		"",
		"[" + t.EmailCopyBtn + "]",
		"",
		fmt.Sprintf(t.EmailValidForFmt, int(ttl.Minutes())),
		"",
		senderName(m.cfg.From, locale),
	}, "\r\n")
}

var htmlTpl = template.Must(template.New("code").Parse(`<!doctype html>
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
</html>`))

func (m *Mailer) buildHTML(code string, ttl time.Duration, locale i18n.Locale) string {
	t := i18n.Get(locale)
	var sb strings.Builder
	err := htmlTpl.Execute(&sb, map[string]any{
		"Code":    code,
		"Minutes": int(ttl.Minutes()),
		"Sender":  senderName(m.cfg.From, locale),
		"Lang":    string(locale),
		"Title":   t.HTMLTitle,
		"Hello":   t.EmailHello,
		"CodeFor": t.EmailCodeFor,
		"CopyBtn": t.EmailCopyBtn,
		"ValidFor": fmt.Sprintf(t.EmailValidForFmt, int(ttl.Minutes())),
	})
	if err != nil {
		return ""
	}
	return sb.String()
}

func senderName(from string, locale i18n.Locale) string {
	if open := strings.Index(from, "<"); open > 0 {
		return strings.TrimSpace(from[:open])
	}
	return i18n.Get(locale).EmailSenderFallback
}
