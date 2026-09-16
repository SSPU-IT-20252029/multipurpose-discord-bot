package verify

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"regexp"
	"strings"
	"time"

	"sspu-multipurpose-discord-bot/internal/i18n"
	"sspu-multipurpose-discord-bot/internal/store"
)

var (
	ErrNotActive        = errors.New("verify: no rule matches this email")
	ErrRateLimited      = errors.New("verify: verification limit exceeded, please try again later")
	ErrNoPending        = errors.New("verify: no pending code found, please use /verify first")
	ErrExpired          = errors.New("verify: code expired")
	ErrTooManyAttempts  = errors.New("verify: too many attempts")
	ErrSendFailed       = errors.New("verify: failed to send email")
	ErrEmailAlreadyUsed = errors.New("verify: email is already bound to another user")
	ErrInvalidDomain    = errors.New("verify: invalid email domain for this server")
	ErrMissingConfig    = errors.New("verify: this server is not fully configured yet")
)

type WrongCodeError struct {
	Remaining int
}

func (e *WrongCodeError) Error() string {
	return "verify: wrong code"
}

type Mailer interface {
	SendCode(to, subject, code string, ttl time.Duration, locale i18n.Locale) error
}

type Service struct {
	store  *store.Store
	mailer Mailer
	Now    func() time.Time
}

func New(st *store.Store, m Mailer) *Service {
	return &Service{
		store:  st,
		mailer: m,
		Now:    time.Now,
	}
}

func (s *Service) Start(ctx context.Context, guildID, discordID, email string, locale i18n.Locale) error {
	email = strings.ToLower(strings.TrimSpace(email))

	cfg, ok, err := s.store.GetGuildConfig(ctx, guildID)
	if err != nil {
		return err
	}
	if !ok || cfg.Domain == "" {
		return ErrMissingConfig
	}

	parts := strings.Split(email, "@")
	if len(parts) != 2 || parts[1] != cfg.Domain {
		return ErrInvalidDomain
	}

	_, err = s.resolveRole(ctx, guildID, email, cfg.Mode)
	if err != nil {
		return err
	}

	now := s.Now()

	if existing, ok, err := s.store.GetVerifiedByEmail(ctx, guildID, email); err == nil && ok && existing.DiscordID != discordID {
		return ErrEmailAlreadyUsed
	}

	sent, err := s.store.CountSendsSince(ctx, guildID, discordID, now.Add(-cfg.RateLimitWindow))
	if err != nil {
		return err
	}
	if sent >= cfg.RateLimitCount {
		return ErrRateLimited
	}

	code, err := generateCode()
	if err != nil {
		return err
	}

	err = s.store.UpsertPending(ctx, store.Pending{
		GuildID:   guildID,
		DiscordID: discordID,
		Email:     email,
		CodeHash:  hash(code),
		ExpiresAt: now.Add(cfg.CodeTTL),
		Attempts:  0,
	})
	if err != nil {
		return err
	}

	if err := s.mailer.SendCode(email, cfg.Subject, code, cfg.CodeTTL, locale); err != nil {
		return errors.Join(ErrSendFailed, err)
	}

	return s.store.LogSend(ctx, guildID, discordID, now)
}

func (s *Service) Confirm(ctx context.Context, guildID, discordID, code string) ([]string, error) {
	pending, ok, err := s.store.GetPending(ctx, guildID, discordID)
	if err != nil {
		return nil, err
	}
	if !ok {
		return nil, ErrNoPending
	}

	cfg, ok, err := s.store.GetGuildConfig(ctx, guildID)
	if err != nil {
		return nil, err
	}
	if !ok {
		return nil, ErrMissingConfig
	}

	now := s.Now()
	if now.After(pending.ExpiresAt) {
		_ = s.store.DeletePending(ctx, guildID, discordID)
		return nil, ErrExpired
	}

	if subtle.ConstantTimeCompare([]byte(hash(normalizeCode(code))), []byte(pending.CodeHash)) != 1 {
		attempts, err := s.store.IncrementAttempts(ctx, guildID, discordID)
		if err != nil {
			return nil, err
		}
		if attempts >= cfg.MaxAttempts {
			_ = s.store.DeletePending(ctx, guildID, discordID)
			return nil, ErrTooManyAttempts
		}
		return nil, &WrongCodeError{Remaining: cfg.MaxAttempts - attempts}
	}

	roleID, err := s.resolveRole(ctx, guildID, pending.Email, cfg.Mode)
	if err != nil {
		_ = s.store.DeletePending(ctx, guildID, discordID)
		return nil, err
	}

	if err := s.store.SetVerified(ctx, guildID, discordID, pending.Email, roleID); err != nil {
		return nil, err
	}
	_ = s.store.DeletePending(ctx, guildID, discordID)

	roles := []string{roleID}
	if cfg.DefaultRoleID != "" && cfg.DefaultRoleID != roleID {
		roles = append(roles, cfg.DefaultRoleID)
	}
	return roles, nil
}

func (s *Service) resolveRole(ctx context.Context, guildID, email, mode string) (string, error) {
	if mode == "REGEX" {
		rules, err := s.store.ListRegexRules(ctx, guildID)
		if err != nil {
			return "", err
		}
		for _, rule := range rules {
			matched, err := regexp.MatchString(rule.Pattern, email)
			if err != nil || !matched {
				continue
			}
			return rule.RoleID, nil
		}
		return "", ErrNotActive
	} else if mode == "CSV" {
		roleID, ok, err := s.store.GetRoleByCSVEmail(ctx, guildID, email)
		if err != nil {
			return "", err
		}
		if !ok {
			return "", ErrNotActive
		}
		return roleID, nil
	}
	return "", ErrMissingConfig
}

func generateCode() (string, error) {
	n, err := rand.Int(rand.Reader, big.NewInt(1_000_000))
	if err != nil {
		return "", fmt.Errorf("generating code: %w", err)
	}
	return fmt.Sprintf("%06d", n.Int64()), nil
}

func normalizeCode(code string) string {
	out := make([]rune, 0, len(code))
	for _, r := range code {
		if r != ' ' {
			out = append(out, r)
		}
	}
	return string(out)
}

func hash(code string) string {
	sum := sha256.Sum256([]byte(code))
	return hex.EncodeToString(sum[:])
}
