package store

import (
	"context"
	"database/sql"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	_ "modernc.org/sqlite"
)

type GuildConfig struct {
	GuildID           string
	VerifyChannelID   string
	Domain            string
	Mode              string // 'REGEX' or 'CSV'
	Subject           string
	CodeTTL           time.Duration
	MaxAttempts       int
	RateLimitCount    int
	RateLimitWindow   time.Duration
	DefaultRoleID     string
}

type RegexRule struct {
	ID       int
	GuildID  string
	Pattern  string
	RoleID   string
	Priority int
}

type VerifiedUser struct {
	GuildID    string
	DiscordID  string
	Email      string
	RoleID     string
	VerifiedAt time.Time
}

type Pending struct {
	GuildID   string
	DiscordID string
	Email     string
	CodeHash  string
	ExpiresAt time.Time
	Attempts  int
}

type Store struct {
	db *sql.DB
}

// BackupRecord mirrors the `backups` table.
type BackupRecord struct {
	ID           int
	GuildID      string
	Scope        string
	Kind         string
	Filepath     string
	CreatedAt    time.Time
	ChannelCount int
	RoleCount    int
	EmojiCount   int
	BanCount     int
}

// ScheduledBackupConfig mirrors the `scheduled_backups` table.
type ScheduledBackupConfig struct {
	GuildID    string
	Enabled    bool
	Frequency  string
	TimeOfDay  string
	NextRun    time.Time
	SlotCount  int
}

func Open(dsn string) (*Store, error) {
	if dir := filepath.Dir(dsn); dir != "" && dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return nil, fmt.Errorf("creating database directory: %w", err)
		}
	}
	db, err := sql.Open("sqlite", dsn+"?_pragma=busy_timeout(5000)&_pragma=journal_mode(WAL)&_pragma=foreign_keys(1)")
	if err != nil {
		return nil, fmt.Errorf("opening database: %w", err)
	}
	db.SetMaxOpenConns(1)
	s := &Store{db: db}
	if err := s.migrate(); err != nil {
		db.Close()
		return nil, err
	}
	return s, nil
}

func (s *Store) migrate() error {
	schema := `
CREATE TABLE IF NOT EXISTS guilds (
	guild_id            TEXT PRIMARY KEY,
	verify_channel_id   TEXT,
	domain              TEXT,
	mode                TEXT,
	subject             TEXT,
	code_ttl            INTEGER,
	max_attempts        INTEGER,
	rate_limit_count    INTEGER NOT NULL DEFAULT 3,
	rate_limit_window   INTEGER NOT NULL DEFAULT 15,
	default_role_id     TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS regex_rules (
	id       INTEGER PRIMARY KEY AUTOINCREMENT,
	guild_id TEXT REFERENCES guilds(guild_id) ON DELETE CASCADE,
	pattern  TEXT NOT NULL,
	role_id  TEXT NOT NULL,
	priority INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS csv_mappings (
	id         INTEGER PRIMARY KEY AUTOINCREMENT,
	guild_id   TEXT REFERENCES guilds(guild_id) ON DELETE CASCADE,
	class_name TEXT NOT NULL,
	role_id    TEXT NOT NULL,
	UNIQUE(guild_id, class_name)
);
CREATE TABLE IF NOT EXISTS csv_emails (
	id         INTEGER PRIMARY KEY AUTOINCREMENT,
	guild_id   TEXT REFERENCES guilds(guild_id) ON DELETE CASCADE,
	email      TEXT NOT NULL,
	class_name TEXT NOT NULL,
	UNIQUE(guild_id, email)
);
CREATE TABLE IF NOT EXISTS verified_users (
	guild_id    TEXT NOT NULL,
	discord_id  TEXT NOT NULL,
	email       TEXT NOT NULL,
	role_id     TEXT NOT NULL,
	verified_at INTEGER NOT NULL,
	PRIMARY KEY (guild_id, discord_id),
	UNIQUE (guild_id, email)
);
CREATE TABLE IF NOT EXISTS pending_codes (
	guild_id   TEXT NOT NULL,
	discord_id TEXT NOT NULL,
	email      TEXT NOT NULL,
	code_hash  TEXT NOT NULL,
	expires_at INTEGER NOT NULL,
	attempts   INTEGER NOT NULL DEFAULT 0,
	PRIMARY KEY (guild_id, discord_id)
);
CREATE TABLE IF NOT EXISTS send_log (
	guild_id   TEXT NOT NULL,
	discord_id TEXT NOT NULL,
	sent_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_send_log_user_time ON send_log(guild_id, discord_id, sent_at);
CREATE TABLE IF NOT EXISTS user_locales (
 	guild_id   TEXT NOT NULL,
 	user_id    TEXT NOT NULL,
 	locale     TEXT NOT NULL DEFAULT 'en',
 	PRIMARY KEY (guild_id, user_id)
 );
 CREATE TABLE IF NOT EXISTS backups (
 	id               INTEGER PRIMARY KEY AUTOINCREMENT,
 	guild_id         TEXT NOT NULL,
 	scope            TEXT NOT NULL DEFAULT 'single',
 	kind             TEXT NOT NULL DEFAULT 'manual',
 	filepath         TEXT NOT NULL,
 	created_at       INTEGER NOT NULL,
 	channel_count    INTEGER NOT NULL DEFAULT 0,
 	role_count       INTEGER NOT NULL DEFAULT 0,
 	emoji_count      INTEGER NOT NULL DEFAULT 0,
 	ban_count        INTEGER NOT NULL DEFAULT 0
 );
 CREATE TABLE IF NOT EXISTS scheduled_backups (
 	guild_id    TEXT PRIMARY KEY,
 	enabled     INTEGER NOT NULL DEFAULT 0,
 	frequency   TEXT NOT NULL DEFAULT '',
 	time_of_day TEXT NOT NULL DEFAULT '',
 	next_run    INTEGER NOT NULL DEFAULT 0,
 	slot_count  INTEGER NOT NULL DEFAULT 3
 );
`
	// Enable foreign keys
	_, err := s.db.Exec("PRAGMA foreign_keys = ON;")
	if err != nil {
		return err
	}
	_, err = s.db.Exec(schema)
	if err != nil {
		return fmt.Errorf("database migration: %w", err)
	}
	if err := s.migrateRateLimit(); err != nil {
		return fmt.Errorf("rate limit migration: %w", err)
	}
	if err := s.migrateDefaultRole(); err != nil {
		return fmt.Errorf("default role migration: %w", err)
	}
	return nil
}

func (s *Store) migrateDefaultRole() error {
	_, err := s.db.Exec(`ALTER TABLE guilds ADD COLUMN default_role_id TEXT NOT NULL DEFAULT ''`)
	if err != nil && !strings.Contains(err.Error(), "duplicate column name") {
		return err
	}
	return nil
}

func (s *Store) migrateRateLimit() error {
	_, err := s.db.Exec(`ALTER TABLE guilds ADD COLUMN rate_limit_count INTEGER NOT NULL DEFAULT 3`)
	if err != nil && !strings.Contains(err.Error(), "duplicate column name") {
		return err
	}
	_, err = s.db.Exec(`ALTER TABLE guilds ADD COLUMN rate_limit_window INTEGER NOT NULL DEFAULT 15`)
	if err != nil && !strings.Contains(err.Error(), "duplicate column name") {
		return err
	}

	var hasOldColumn bool
	err = s.db.QueryRow(`SELECT COUNT(*) FROM pragma_table_info('guilds') WHERE name = 'rate_limit_per_hour'`).Scan(&hasOldColumn)
	if err != nil {
		return err
	}

	if hasOldColumn {
		_, err = s.db.Exec(`UPDATE guilds SET rate_limit_count = rate_limit_per_hour, rate_limit_window = 15 WHERE rate_limit_per_hour IS NOT NULL AND rate_limit_count = 3 AND rate_limit_window = 15`)
		if err != nil {
			return err
		}
	}

	_, err = s.db.Exec(`UPDATE guilds SET rate_limit_count = COALESCE(NULLIF(rate_limit_count, 0), 3), rate_limit_window = COALESCE(NULLIF(rate_limit_window, 0), 15) WHERE rate_limit_count IS NULL OR rate_limit_window IS NULL`)
	if err != nil {
		return err
	}

	return nil
}

func (s *Store) Close() error {
	return s.db.Close()
}

// Guild Config
func (s *Store) SaveGuildConfig(ctx context.Context, g GuildConfig) error {
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO guilds (guild_id, verify_channel_id, domain, mode, subject, code_ttl, max_attempts, rate_limit_count, rate_limit_window, default_role_id)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		 ON CONFLICT(guild_id) DO UPDATE SET 
		 verify_channel_id=excluded.verify_channel_id,
		 domain=excluded.domain,
		 mode=excluded.mode,
		 subject=excluded.subject,
		 code_ttl=excluded.code_ttl,
		 max_attempts=excluded.max_attempts,
		 rate_limit_count=excluded.rate_limit_count,
		 rate_limit_window=excluded.rate_limit_window,
		 default_role_id=excluded.default_role_id`,
		g.GuildID, g.VerifyChannelID, g.Domain, g.Mode, g.Subject, int64(g.CodeTTL), g.MaxAttempts, g.RateLimitCount, int64(g.RateLimitWindow), g.DefaultRoleID)
	return err
}

func (s *Store) GetGuildConfig(ctx context.Context, guildID string) (GuildConfig, bool, error) {
	var g GuildConfig
	var ttl, window int64
	err := s.db.QueryRowContext(ctx,
		`SELECT guild_id, verify_channel_id, domain, mode, subject, code_ttl, max_attempts, rate_limit_count, rate_limit_window, default_role_id
		 FROM guilds WHERE guild_id = ?`, guildID).
		Scan(&g.GuildID, &g.VerifyChannelID, &g.Domain, &g.Mode, &g.Subject, &ttl, &g.MaxAttempts, &g.RateLimitCount, &window, &g.DefaultRoleID)
	if err == sql.ErrNoRows {
		return GuildConfig{}, false, nil
	}
	if err != nil {
		return GuildConfig{}, false, err
	}
	g.CodeTTL = time.Duration(ttl)
	g.RateLimitWindow = time.Duration(window)
	return g, true, nil
}

func (s *Store) ListGuildConfigs(ctx context.Context) ([]GuildConfig, error) {
	rows, err := s.db.QueryContext(ctx, `SELECT guild_id, verify_channel_id, domain, mode, subject, code_ttl, max_attempts, rate_limit_count, rate_limit_window, default_role_id FROM guilds`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []GuildConfig
	for rows.Next() {
		var g GuildConfig
		var ttl, window int64
		if err := rows.Scan(&g.GuildID, &g.VerifyChannelID, &g.Domain, &g.Mode, &g.Subject, &ttl, &g.MaxAttempts, &g.RateLimitCount, &window, &g.DefaultRoleID); err != nil {
			return nil, err
		}
		g.CodeTTL = time.Duration(ttl)
		g.RateLimitWindow = time.Duration(window)
		out = append(out, g)
	}
	return out, nil
}

// Regex Rules
func (s *Store) AddRegexRule(ctx context.Context, r RegexRule) error {
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO regex_rules (guild_id, pattern, role_id, priority) VALUES (?, ?, ?, ?)`,
		r.GuildID, r.Pattern, r.RoleID, r.Priority)
	if err != nil {
		return err
	}
	return s.ReorderRegexPriorities(ctx, r.GuildID)
}

func (s *Store) RemoveRegexRule(ctx context.Context, id int) error {
	var guildID string
	err := s.db.QueryRowContext(ctx, `SELECT guild_id FROM regex_rules WHERE id = ?`, id).Scan(&guildID)
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(ctx, `DELETE FROM regex_rules WHERE id = ?`, id)
	if err != nil {
		return err
	}
	return s.ReorderRegexPriorities(ctx, guildID)
}

func (s *Store) ListRegexRules(ctx context.Context, guildID string) ([]RegexRule, error) {
	rows, err := s.db.QueryContext(ctx,
		`SELECT id, guild_id, pattern, role_id, priority FROM regex_rules WHERE guild_id = ? ORDER BY priority DESC`, guildID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []RegexRule
	for rows.Next() {
		var r RegexRule
		if err := rows.Scan(&r.ID, &r.GuildID, &r.Pattern, &r.RoleID, &r.Priority); err != nil {
			return nil, err
		}
		out = append(out, r)
	}
	return out, nil
}

func (s *Store) RemoveAllRegexRules(ctx context.Context, guildID string) error {
	_, err := s.db.ExecContext(ctx, `DELETE FROM regex_rules WHERE guild_id = ?`, guildID)
	return err
}

func (s *Store) RemoveRegexRulesRange(ctx context.Context, guildID string, startID, endID int) error {
	_, err := s.db.ExecContext(ctx, `DELETE FROM regex_rules WHERE guild_id = ? AND id BETWEEN ? AND ?`, guildID, startID, endID)
	if err != nil {
		return err
	}
	return s.ReorderRegexPriorities(ctx, guildID)
}

func (s *Store) ReorderRegexPriorities(ctx context.Context, guildID string) error {
	rules, err := s.ListRegexRules(ctx, guildID)
	if err != nil {
		return err
	}
	
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	
	for i, rule := range rules {
		newPriority := len(rules) - i
		_, err = tx.ExecContext(ctx, `UPDATE regex_rules SET priority = ? WHERE id = ?`, newPriority, rule.ID)
		if err != nil {
			return err
		}
	}
	
	return tx.Commit()
}

func (s *Store) BulkInsertRegexRules(ctx context.Context, guildID string, rules []RegexRule) error {
	if len(rules) == 0 {
		return nil
	}
	
	tx, err := s.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	
	stmt, err := tx.PrepareContext(ctx, `INSERT INTO regex_rules (guild_id, pattern, role_id, priority) VALUES (?, ?, ?, ?)`)
	if err != nil {
		return err
	}
	defer stmt.Close()
	
	for _, rule := range rules {
		_, err = stmt.ExecContext(ctx, rule.GuildID, rule.Pattern, rule.RoleID, rule.Priority)
		if err != nil {
			return err
		}
	}
	
	if err := tx.Commit(); err != nil {
		return err
	}
	
	return s.ReorderRegexPriorities(ctx, guildID)
}

// CSV Data
func (s *Store) ClearCSVEmails(ctx context.Context, guildID string) error {
	_, err := s.db.ExecContext(ctx, `DELETE FROM csv_emails WHERE guild_id = ?`, guildID)
	return err
}

func (s *Store) InsertCSVEmail(ctx context.Context, guildID, email, className string) error {
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO csv_emails (guild_id, email, class_name) VALUES (?, ?, ?)
		 ON CONFLICT(guild_id, email) DO UPDATE SET class_name=excluded.class_name`,
		guildID, email, className)
	return err
}

func (s *Store) MapCSVClass(ctx context.Context, guildID, className, roleID string) error {
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO csv_mappings (guild_id, class_name, role_id) VALUES (?, ?, ?)
		 ON CONFLICT(guild_id, class_name) DO UPDATE SET role_id=excluded.role_id`,
		guildID, className, roleID)
	return err
}

func (s *Store) UnmapCSVClass(ctx context.Context, guildID, className string) error {
	_, err := s.db.ExecContext(ctx, `DELETE FROM csv_mappings WHERE guild_id = ? AND class_name = ?`, guildID, className)
	return err
}

func (s *Store) GetRoleByCSVEmail(ctx context.Context, guildID, email string) (string, bool, error) {
	var roleID string
	err := s.db.QueryRowContext(ctx,
		`SELECT m.role_id 
		 FROM csv_emails e 
		 JOIN csv_mappings m ON e.guild_id = m.guild_id AND e.class_name = m.class_name
		 WHERE e.guild_id = ? AND e.email = ?`, guildID, email).Scan(&roleID)
	if err == sql.ErrNoRows {
		return "", false, nil
	}
	if err != nil {
		return "", false, err
	}
	return roleID, true, nil
}

// Verified Users
func (s *Store) SetVerified(ctx context.Context, guildID, discordID, email, roleID string) error {
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO verified_users (guild_id, discord_id, email, role_id, verified_at) VALUES (?, ?, ?, ?, ?)
		 ON CONFLICT(guild_id, discord_id) DO UPDATE SET email = excluded.email, role_id = excluded.role_id, verified_at = excluded.verified_at`,
		guildID, discordID, email, roleID, time.Now().Unix())
	return err
}

func (s *Store) GetVerifiedByEmail(ctx context.Context, guildID, email string) (VerifiedUser, bool, error) {
	var v VerifiedUser
	var at int64
	err := s.db.QueryRowContext(ctx,
		`SELECT guild_id, discord_id, email, role_id, verified_at FROM verified_users WHERE guild_id = ? AND email = ?`, guildID, email).
		Scan(&v.GuildID, &v.DiscordID, &v.Email, &v.RoleID, &at)
	if err == sql.ErrNoRows {
		return VerifiedUser{}, false, nil
	}
	if err != nil {
		return VerifiedUser{}, false, err
	}
	v.VerifiedAt = time.Unix(at, 0)
	return v, true, nil
}

// Pending Codes
func (s *Store) UpsertPending(ctx context.Context, p Pending) error {
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO pending_codes (guild_id, discord_id, email, code_hash, expires_at, attempts) VALUES (?, ?, ?, ?, ?, ?)
		 ON CONFLICT(guild_id, discord_id) DO UPDATE SET email = excluded.email, code_hash = excluded.code_hash,
		   expires_at = excluded.expires_at, attempts = excluded.attempts`,
		p.GuildID, p.DiscordID, p.Email, p.CodeHash, p.ExpiresAt.Unix(), p.Attempts)
	return err
}

func (s *Store) GetPending(ctx context.Context, guildID, discordID string) (Pending, bool, error) {
	var p Pending
	var exp int64
	err := s.db.QueryRowContext(ctx,
		`SELECT guild_id, discord_id, email, code_hash, expires_at, attempts FROM pending_codes WHERE guild_id = ? AND discord_id = ?`, guildID, discordID).
		Scan(&p.GuildID, &p.DiscordID, &p.Email, &p.CodeHash, &exp, &p.Attempts)
	if err == sql.ErrNoRows {
		return Pending{}, false, nil
	}
	if err != nil {
		return Pending{}, false, err
	}
	p.ExpiresAt = time.Unix(exp, 0)
	return p, true, nil
}

func (s *Store) DeletePending(ctx context.Context, guildID, discordID string) error {
	_, err := s.db.ExecContext(ctx, `DELETE FROM pending_codes WHERE guild_id = ? AND discord_id = ?`, guildID, discordID)
	return err
}

func (s *Store) IncrementAttempts(ctx context.Context, guildID, discordID string) (int, error) {
	var attempts int
	err := s.db.QueryRowContext(ctx,
		`UPDATE pending_codes SET attempts = attempts + 1 WHERE guild_id = ? AND discord_id = ? RETURNING attempts`, guildID, discordID).
		Scan(&attempts)
	if err != nil {
		return 0, err
	}
	return attempts, nil
}

// Rate Limiting
func (s *Store) LogSend(ctx context.Context, guildID, discordID string, at time.Time) error {
	_, err := s.db.ExecContext(ctx, `INSERT INTO send_log (guild_id, discord_id, sent_at) VALUES (?, ?, ?)`, guildID, discordID, at.Unix())
	if err != nil {
		return err
	}
	_, err = s.db.ExecContext(ctx, `DELETE FROM send_log WHERE sent_at < ?`, at.Add(-2*time.Hour).Unix())
	return err
}

func (s *Store) CountSendsSince(ctx context.Context, guildID, discordID string, since time.Time) (int, error) {
	var n int
	err := s.db.QueryRowContext(ctx,
		"SELECT COUNT(*) FROM send_log WHERE guild_id = ? AND discord_id = ? AND sent_at >= ?", guildID, discordID, since.Unix()).Scan(&n)
	return n, err
}

func (s *Store) GetUserLocale(ctx context.Context, guildID, userID string) (string, bool, error) {
	var locale string
	err := s.db.QueryRowContext(ctx,
		"SELECT locale FROM user_locales WHERE guild_id = ? AND user_id = ?", guildID, userID).Scan(&locale)
	if err == sql.ErrNoRows {
		return "", false, nil
	}
	if err != nil {
		return "", false, err
	}
	return locale, true, nil
}

func (s *Store) SetUserLocale(ctx context.Context, guildID, userID, locale string) error {
	_, err := s.db.ExecContext(ctx,
		"INSERT INTO user_locales (guild_id, user_id, locale) VALUES (?, ?, ?) ON CONFLICT(guild_id, user_id) DO UPDATE SET locale = excluded.locale",
		guildID, userID, locale)
	return err
}

// Backup Records

func (s *Store) SaveBackup(ctx context.Context, r BackupRecord) (int64, error) {
	res, err := s.db.ExecContext(ctx,
		`INSERT INTO backups (guild_id, scope, kind, filepath, created_at, channel_count, role_count, emoji_count, ban_count)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		r.GuildID, r.Scope, r.Kind, r.Filepath, r.CreatedAt.Unix(), r.ChannelCount, r.RoleCount, r.EmojiCount, r.BanCount)
	if err != nil {
		return 0, err
	}
	return res.LastInsertId()
}

func (s *Store) ListBackups(ctx context.Context, guildID, kind string) ([]BackupRecord, error) {
	var args []interface{}
	query := "SELECT id, guild_id, scope, kind, filepath, created_at, channel_count, role_count, emoji_count, ban_count FROM backups WHERE 1=1"
	if guildID != "" {
		query += " AND guild_id = ?"
		args = append(args, guildID)
	}
	if kind != "" {
		query += " AND kind = ?"
		args = append(args, kind)
	}
	query += " ORDER BY created_at DESC"
	rows, err := s.db.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []BackupRecord
	for rows.Next() {
		var r BackupRecord
		var ts int64
		if err := rows.Scan(&r.ID, &r.GuildID, &r.Scope, &r.Kind, &r.Filepath, &ts, &r.ChannelCount, &r.RoleCount, &r.EmojiCount, &r.BanCount); err != nil {
			return nil, err
		}
		r.CreatedAt = time.Unix(ts, 0)
		out = append(out, r)
	}
	return out, nil
}

func (s *Store) GetBackup(ctx context.Context, id int) (BackupRecord, bool, error) {
	var r BackupRecord
	var ts int64
	err := s.db.QueryRowContext(ctx,
		`SELECT id, guild_id, scope, kind, filepath, created_at, channel_count, role_count, emoji_count, ban_count FROM backups WHERE id = ?`, id).
		Scan(&r.ID, &r.GuildID, &r.Scope, &r.Kind, &r.Filepath, &ts, &r.ChannelCount, &r.RoleCount, &r.EmojiCount, &r.BanCount)
	if err == sql.ErrNoRows {
		return BackupRecord{}, false, nil
	}
	if err != nil {
		return BackupRecord{}, false, err
	}
	r.CreatedAt = time.Unix(ts, 0)
	return r, true, nil
}

func (s *Store) DeleteBackup(ctx context.Context, id int) error {
	_, err := s.db.ExecContext(ctx, `DELETE FROM backups WHERE id = ?`, id)
	return err
}

// Scheduled Backup Config

func (s *Store) GetScheduledConfig(ctx context.Context, guildID string) (ScheduledBackupConfig, bool, error) {
	var c ScheduledBackupConfig
	var enabled int
	var nextRun int64
	err := s.db.QueryRowContext(ctx,
		`SELECT guild_id, enabled, frequency, time_of_day, next_run, slot_count FROM scheduled_backups WHERE guild_id = ?`, guildID).
		Scan(&c.GuildID, &enabled, &c.Frequency, &c.TimeOfDay, &nextRun, &c.SlotCount)
	if err == sql.ErrNoRows {
		return ScheduledBackupConfig{}, false, nil
	}
	if err != nil {
		return ScheduledBackupConfig{}, false, err
	}
	c.Enabled = enabled != 0
	c.NextRun = time.Unix(nextRun, 0)
	return c, true, nil
}

func (s *Store) SaveScheduledConfig(ctx context.Context, c ScheduledBackupConfig) error {
	enabled := 0
	if c.Enabled {
		enabled = 1
	}
	_, err := s.db.ExecContext(ctx,
		`INSERT INTO scheduled_backups (guild_id, enabled, frequency, time_of_day, next_run, slot_count)
		 VALUES (?, ?, ?, ?, ?, ?)
		 ON CONFLICT(guild_id) DO UPDATE SET
		 enabled=excluded.enabled, frequency=excluded.frequency, time_of_day=excluded.time_of_day,
		 next_run=excluded.next_run, slot_count=excluded.slot_count`,
		c.GuildID, enabled, c.Frequency, c.TimeOfDay, c.NextRun.Unix(), c.SlotCount)
	return err
}

func (s *Store) ListScheduledBackups(ctx context.Context) ([]ScheduledBackupConfig, error) {
	rows, err := s.db.QueryContext(ctx, `SELECT guild_id, enabled, frequency, time_of_day, next_run, slot_count FROM scheduled_backups WHERE enabled = 1`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []ScheduledBackupConfig
	for rows.Next() {
		var c ScheduledBackupConfig
		var enabled int
		var nextRun int64
		if err := rows.Scan(&c.GuildID, &enabled, &c.Frequency, &c.TimeOfDay, &nextRun, &c.SlotCount); err != nil {
			return nil, err
		}
		c.Enabled = enabled != 0
		c.NextRun = time.Unix(nextRun, 0)
		out = append(out, c)
	}
	return out, nil
}
