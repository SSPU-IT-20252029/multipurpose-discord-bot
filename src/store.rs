//! SQLite persistence layer.
//!
//! Parity target: Go `internal/store/store.go`. The DDL, pragmas, migrations,
//! and every statement are kept byte-identical so a database produced by the
//! Go bot opens unchanged and vice-versa.

use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(rusqlite::Error),
    #[error("foreign key violation — run /setup first")]
    ForeignKey,
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

const SQLITE_CONSTRAINT_FOREIGNKEY: i32 = 787;

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        match &e {
            rusqlite::Error::SqliteFailure(err, _)
                if err.extended_code == SQLITE_CONSTRAINT_FOREIGNKEY =>
            {
                StoreError::ForeignKey
            }
            _ => StoreError::Db(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Data models
// ---------------------------------------------------------------------------

/// Mirrors Go `store.GuildConfig`. Durations are stored as nanoseconds
/// (Go `time.Duration` stored via `int64(...)`).
#[derive(Debug, Clone, PartialEq)]
pub struct GuildConfig {
    pub guild_id: String,
    pub verify_channel_id: String,
    pub domain: String,
    /// `"REGEX"` or `"CSV"`.
    pub mode: String,
    pub subject: String,
    pub code_ttl_ns: i64,
    pub max_attempts: i64,
    pub rate_limit_count: i64,
    pub rate_limit_window_ns: i64,
    pub default_role_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegexRule {
    pub id: i64,
    pub guild_id: String,
    pub pattern: String,
    pub role_id: String,
    pub priority: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedUser {
    pub guild_id: String,
    pub discord_id: String,
    pub email: String,
    pub role_id: String,
    pub verified_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PendingCode {
    pub guild_id: String,
    pub discord_id: String,
    pub email: String,
    pub code_hash: String,
    pub expires_at: i64,
    pub attempts: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackupRecord {
    pub id: i64,
    pub guild_id: String,
    pub scope: String,
    pub kind: String,
    pub filepath: String,
    pub created_at: i64,
    pub channel_count: i64,
    pub role_count: i64,
    pub emoji_count: i64,
    pub ban_count: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledBackup {
    pub guild_id: String,
    pub enabled: bool,
    pub frequency: String,
    pub time_of_day: String,
    pub next_run: i64,
    pub slot_count: i64,
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/// Single-writer SQLite store, shared via `Arc`. Mirrors Go's
/// `SetMaxOpenConns(1)`.
#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    /// Open (creating the parent directory and schema as needed) a database.
    ///
    /// Parity: Go `store.Open` — `MkdirAll(dir, 0o755)`, pragmas
    /// `busy_timeout(5000)`, `journal_mode(WAL)`, `foreign_keys(1)`, one
    /// connection, then `migrate()`.
    pub fn open(dsn: &str) -> Result<Self, StoreError> {
        if let Some(dir) = Path::new(dsn).parent()
            && !dir.as_os_str().is_empty()
            && dir != Path::new(".")
        {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(dsn)?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Wrap an existing connection without running migrations (test helper).
    #[cfg(test)]
    pub(crate) fn from_connection(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("store mutex poisoned")
    }

    fn now_unix_secs() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Schema + migrations
// ---------------------------------------------------------------------------

/// Byte-identical to the Go schema (nullable TEXT/INTEGER columns and all).
const SCHEMA: &str = r#"
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
"#;

impl Store {
    /// `pub(crate)` so cross-module unit tests (e.g. `verify`) can build an
    /// in-memory schema without touching disk.
    pub(crate) fn migrate(&self) -> Result<(), StoreError> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute_batch(SCHEMA)?;
        Self::migrate_rate_limit(&tx)?;
        Self::migrate_default_role(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Parity: Go `migrateRateLimit` — idempotent `ADD COLUMN` (ignoring
    /// "duplicate column name"), legacy `rate_limit_per_hour` backfill, and
    /// NULL/zero backfill to defaults.
    fn migrate_rate_limit(tx: &rusqlite::Transaction<'_>) -> Result<(), StoreError> {
        let add = |sql: &str, tx: &rusqlite::Transaction<'_>| -> Result<(), StoreError> {
            match tx.execute_batch(sql) {
                Ok(()) => Ok(()),
                Err(e) if is_duplicate_column(&e) => Ok(()),
                Err(e) => Err(e.into()),
            }
        };
        add(
            "ALTER TABLE guilds ADD COLUMN rate_limit_count INTEGER NOT NULL DEFAULT 3",
            tx,
        )?;
        add(
            "ALTER TABLE guilds ADD COLUMN rate_limit_window INTEGER NOT NULL DEFAULT 15",
            tx,
        )?;

        let has_old: i64 = tx.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('guilds') WHERE name = 'rate_limit_per_hour'",
            [],
            |row| row.get(0),
        )?;
        if has_old > 0 {
            tx.execute_batch(
                "UPDATE guilds SET rate_limit_count = rate_limit_per_hour, rate_limit_window = 15 \
                 WHERE rate_limit_per_hour IS NOT NULL AND rate_limit_count = 3 AND rate_limit_window = 15",
            )?;
        }

        tx.execute_batch(
            "UPDATE guilds SET rate_limit_count = COALESCE(NULLIF(rate_limit_count, 0), 3), \
             rate_limit_window = COALESCE(NULLIF(rate_limit_window, 0), 15) \
             WHERE rate_limit_count IS NULL OR rate_limit_window IS NULL",
        )?;
        Ok(())
    }

    /// Parity: Go `migrateDefaultRole` — idempotent `ADD COLUMN`.
    fn migrate_default_role(tx: &rusqlite::Transaction<'_>) -> Result<(), StoreError> {
        match tx
            .execute_batch("ALTER TABLE guilds ADD COLUMN default_role_id TEXT NOT NULL DEFAULT ''")
        {
            Ok(()) => Ok(()),
            Err(e) if is_duplicate_column(&e) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

fn is_duplicate_column(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(_, Some(msg)) if msg.contains("duplicate column name")
    )
}

// ---------------------------------------------------------------------------
// Guild config
// ---------------------------------------------------------------------------

impl Store {
    pub fn save_guild_config(&self, g: &GuildConfig) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO guilds (guild_id, verify_channel_id, domain, mode, subject, code_ttl, max_attempts, rate_limit_count, rate_limit_window, default_role_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(guild_id) DO UPDATE SET
               verify_channel_id=excluded.verify_channel_id,
               domain=excluded.domain,
               mode=excluded.mode,
               subject=excluded.subject,
               code_ttl=excluded.code_ttl,
               max_attempts=excluded.max_attempts,
               rate_limit_count=excluded.rate_limit_count,
               rate_limit_window=excluded.rate_limit_window,
               default_role_id=excluded.default_role_id",
            params![
                g.guild_id,
                g.verify_channel_id,
                g.domain,
                g.mode,
                g.subject,
                g.code_ttl_ns,
                g.max_attempts,
                g.rate_limit_count,
                g.rate_limit_window_ns,
                g.default_role_id
            ],
        )?;
        Ok(())
    }

    pub fn get_guild_config(&self, guild_id: &str) -> Result<Option<GuildConfig>, StoreError> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT guild_id, verify_channel_id, domain, mode, subject, code_ttl, max_attempts, rate_limit_count, rate_limit_window, default_role_id
                 FROM guilds WHERE guild_id = ?1",
                params![guild_id],
                map_guild_config,
            )
            .optional()?;
        Ok(row)
    }

    pub fn list_guild_configs(&self) -> Result<Vec<GuildConfig>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT guild_id, verify_channel_id, domain, mode, subject, code_ttl, max_attempts, rate_limit_count, rate_limit_window, default_role_id
             FROM guilds",
        )?;
        let rows = stmt.query_map([], map_guild_config)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn map_guild_config(row: &Row<'_>) -> rusqlite::Result<GuildConfig> {
    // The DDL makes most columns nullable (parity); NULLs read as Go's zero values.
    Ok(GuildConfig {
        guild_id: row.get(0)?,
        verify_channel_id: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
        domain: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
        mode: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        subject: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        code_ttl_ns: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
        max_attempts: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
        rate_limit_count: row.get(7)?,
        rate_limit_window_ns: row.get(8)?,
        default_role_id: row.get(9)?,
    })
}

// ---------------------------------------------------------------------------
// Regex rules
// ---------------------------------------------------------------------------

impl Store {
    /// Parity: Go `AddRegexRule` writes identical rows. Rust additionally
    /// returns the new rowid (Go discards it); harmless divergence.
    pub fn add_regex_rule(
        &self,
        guild_id: &str,
        pattern: &str,
        role_id: &str,
        priority: i64,
    ) -> Result<i64, StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO regex_rules (guild_id, pattern, role_id, priority) VALUES (?1, ?2, ?3, ?4)",
            params![guild_id, pattern, role_id, priority],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Parity: Go `RemoveRegexRule(id)` — by id only, not guild-scoped.
    pub fn remove_regex_rule(&self, id: i64) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute("DELETE FROM regex_rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_regex_rules(&self, guild_id: &str) -> Result<Vec<RegexRule>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, guild_id, pattern, role_id, priority FROM regex_rules WHERE guild_id = ?1 ORDER BY priority DESC",
        )?;
        let rows = stmt.query_map(params![guild_id], |row| {
            Ok(RegexRule {
                id: row.get(0)?,
                guild_id: row.get(1)?,
                pattern: row.get(2)?,
                role_id: row.get(3)?,
                priority: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

// ---------------------------------------------------------------------------
// CSV data
// ---------------------------------------------------------------------------

impl Store {
    pub fn clear_csv_emails(&self, guild_id: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM csv_emails WHERE guild_id = ?1",
            params![guild_id],
        )?;
        Ok(())
    }

    /// Parity: Go `InsertCSVEmail` uses `ON CONFLICT(guild_id, email) DO UPDATE`.
    pub fn insert_csv_email(
        &self,
        guild_id: &str,
        email: &str,
        class_name: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO csv_emails (guild_id, email, class_name) VALUES (?1, ?2, ?3)
             ON CONFLICT(guild_id, email) DO UPDATE SET class_name=excluded.class_name",
            params![guild_id, email, class_name],
        )?;
        Ok(())
    }

    pub fn map_csv_class(
        &self,
        guild_id: &str,
        class_name: &str,
        role_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO csv_mappings (guild_id, class_name, role_id) VALUES (?1, ?2, ?3)
             ON CONFLICT(guild_id, class_name) DO UPDATE SET role_id=excluded.role_id",
            params![guild_id, class_name, role_id],
        )?;
        Ok(())
    }

    pub fn unmap_csv_class(&self, guild_id: &str, class_name: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM csv_mappings WHERE guild_id = ?1 AND class_name = ?2",
            params![guild_id, class_name],
        )?;
        Ok(())
    }

    /// Parity: Go `GetRoleByCSVEmail` — JOIN across email→class→role.
    pub fn get_role_by_csv_email(
        &self,
        guild_id: &str,
        email: &str,
    ) -> Result<Option<String>, StoreError> {
        let conn = self.conn();
        let role: Option<String> = conn
            .query_row(
                "SELECT m.role_id
                 FROM csv_emails e
                 JOIN csv_mappings m ON e.guild_id = m.guild_id AND e.class_name = m.class_name
                 WHERE e.guild_id = ?1 AND e.email = ?2",
                params![guild_id, email],
                |row| row.get(0),
            )
            .optional()?;
        Ok(role)
    }
}

// ---------------------------------------------------------------------------
// Verified users
// ---------------------------------------------------------------------------

impl Store {
    /// Parity: Go `SetVerified` stamps `time.Now().Unix()` internally.
    pub fn set_verified(
        &self,
        guild_id: &str,
        discord_id: &str,
        email: &str,
        role_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO verified_users (guild_id, discord_id, email, role_id, verified_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(guild_id, discord_id) DO UPDATE SET email = excluded.email, role_id = excluded.role_id, verified_at = excluded.verified_at",
            params![guild_id, discord_id, email, role_id, Self::now_unix_secs()],
        )?;
        Ok(())
    }

    pub fn get_verified_by_email(
        &self,
        guild_id: &str,
        email: &str,
    ) -> Result<Option<VerifiedUser>, StoreError> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT guild_id, discord_id, email, role_id, verified_at FROM verified_users WHERE guild_id = ?1 AND email = ?2",
                params![guild_id, email],
                map_verified_user,
            )
            .optional()?;
        Ok(row)
    }
}

fn map_verified_user(row: &Row<'_>) -> rusqlite::Result<VerifiedUser> {
    Ok(VerifiedUser {
        guild_id: row.get(0)?,
        discord_id: row.get(1)?,
        email: row.get(2)?,
        role_id: row.get(3)?,
        verified_at: row.get(4)?,
    })
}

// ---------------------------------------------------------------------------
// Pending codes
// ---------------------------------------------------------------------------

impl Store {
    pub fn upsert_pending(&self, p: &PendingCode) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO pending_codes (guild_id, discord_id, email, code_hash, expires_at, attempts) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(guild_id, discord_id) DO UPDATE SET email = excluded.email, code_hash = excluded.code_hash,
               expires_at = excluded.expires_at, attempts = excluded.attempts",
            params![p.guild_id, p.discord_id, p.email, p.code_hash, p.expires_at, p.attempts],
        )?;
        Ok(())
    }

    pub fn get_pending(
        &self,
        guild_id: &str,
        discord_id: &str,
    ) -> Result<Option<PendingCode>, StoreError> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT guild_id, discord_id, email, code_hash, expires_at, attempts FROM pending_codes WHERE guild_id = ?1 AND discord_id = ?2",
                params![guild_id, discord_id],
                map_pending_code,
            )
            .optional()?;
        Ok(row)
    }

    pub fn delete_pending(&self, guild_id: &str, discord_id: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM pending_codes WHERE guild_id = ?1 AND discord_id = ?2",
            params![guild_id, discord_id],
        )?;
        Ok(())
    }

    /// Parity: Go `IncrementAttempts` — `UPDATE ... RETURNING attempts`.
    pub fn increment_attempts(&self, guild_id: &str, discord_id: &str) -> Result<i64, StoreError> {
        let conn = self.conn();
        let attempts: i64 = conn.query_row(
            "UPDATE pending_codes SET attempts = attempts + 1 WHERE guild_id = ?1 AND discord_id = ?2 RETURNING attempts",
            params![guild_id, discord_id],
            |row| row.get(0),
        )?;
        Ok(attempts)
    }
}

fn map_pending_code(row: &Row<'_>) -> rusqlite::Result<PendingCode> {
    Ok(PendingCode {
        guild_id: row.get(0)?,
        discord_id: row.get(1)?,
        email: row.get(2)?,
        code_hash: row.get(3)?,
        expires_at: row.get(4)?,
        attempts: row.get(5)?,
    })
}

// ---------------------------------------------------------------------------
// Send log / rate limiting
// ---------------------------------------------------------------------------

impl Store {
    /// Parity: Go `LogSend` inserts the send and prunes rows older than 2h in
    /// the same call.
    pub fn log_send(&self, guild_id: &str, discord_id: &str, at: i64) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO send_log (guild_id, discord_id, sent_at) VALUES (?1, ?2, ?3)",
            params![guild_id, discord_id, at],
        )?;
        conn.execute(
            "DELETE FROM send_log WHERE sent_at < ?1",
            params![at - 2 * 3600],
        )?;
        Ok(())
    }

    pub fn count_sends_since(
        &self,
        guild_id: &str,
        discord_id: &str,
        since: i64,
    ) -> Result<i64, StoreError> {
        let conn = self.conn();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM send_log WHERE guild_id = ?1 AND discord_id = ?2 AND sent_at >= ?3",
            params![guild_id, discord_id, since],
            |row| row.get(0),
        )?;
        Ok(n)
    }
}

// ---------------------------------------------------------------------------
// User locales
// ---------------------------------------------------------------------------

impl Store {
    pub fn get_user_locale(
        &self,
        guild_id: &str,
        user_id: &str,
    ) -> Result<Option<String>, StoreError> {
        let conn = self.conn();
        let locale: Option<String> = conn
            .query_row(
                "SELECT locale FROM user_locales WHERE guild_id = ?1 AND user_id = ?2",
                params![guild_id, user_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(locale)
    }

    pub fn set_user_locale(
        &self,
        guild_id: &str,
        user_id: &str,
        locale: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO user_locales (guild_id, user_id, locale) VALUES (?1, ?2, ?3)
             ON CONFLICT(guild_id, user_id) DO UPDATE SET locale = excluded.locale",
            params![guild_id, user_id, locale],
        )?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Backup records
// ---------------------------------------------------------------------------

impl Store {
    /// Parity: Go `SaveBackup` returns `LastInsertId()`.
    pub fn save_backup(&self, r: &BackupRecord) -> Result<i64, StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO backups (guild_id, scope, kind, filepath, created_at, channel_count, role_count, emoji_count, ban_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                r.guild_id,
                r.scope,
                r.kind,
                r.filepath,
                r.created_at,
                r.channel_count,
                r.role_count,
                r.emoji_count,
                r.ban_count
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Parity: Go `ListBackups(guildID, kind)` — both filters optional.
    pub fn list_backups(
        &self,
        guild_id: Option<&str>,
        kind: Option<&str>,
    ) -> Result<Vec<BackupRecord>, StoreError> {
        let mut sql = String::from(
            "SELECT id, guild_id, scope, kind, filepath, created_at, channel_count, role_count, emoji_count, ban_count FROM backups WHERE 1=1",
        );
        let mut args: Vec<String> = Vec::new();
        if let Some(g) = guild_id {
            sql.push_str(" AND guild_id = ?");
            args.push(g.to_string());
        }
        if let Some(k) = kind {
            sql.push_str(" AND kind = ?");
            args.push(k.to_string());
        }
        sql.push_str(" ORDER BY created_at DESC");

        let conn = self.conn();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), map_backup_record)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Parity: Go `GetBackup(id)` — by id only, not guild-scoped.
    pub fn get_backup(&self, id: i64) -> Result<Option<BackupRecord>, StoreError> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT id, guild_id, scope, kind, filepath, created_at, channel_count, role_count, emoji_count, ban_count FROM backups WHERE id = ?1",
                params![id],
                map_backup_record,
            )
            .optional()?;
        Ok(row)
    }

    /// Parity: Go `DeleteBackup(id)` — by id only.
    pub fn delete_backup(&self, id: i64) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute("DELETE FROM backups WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn map_backup_record(row: &Row<'_>) -> rusqlite::Result<BackupRecord> {
    Ok(BackupRecord {
        id: row.get(0)?,
        guild_id: row.get(1)?,
        scope: row.get(2)?,
        kind: row.get(3)?,
        filepath: row.get(4)?,
        created_at: row.get(5)?,
        channel_count: row.get(6)?,
        role_count: row.get(7)?,
        emoji_count: row.get(8)?,
        ban_count: row.get(9)?,
    })
}

// ---------------------------------------------------------------------------
// Scheduled backup config
// ---------------------------------------------------------------------------

impl Store {
    pub fn get_scheduled_config(
        &self,
        guild_id: &str,
    ) -> Result<Option<ScheduledBackup>, StoreError> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT guild_id, enabled, frequency, time_of_day, next_run, slot_count FROM scheduled_backups WHERE guild_id = ?1",
                params![guild_id],
                map_scheduled_backup,
            )
            .optional()?;
        Ok(row)
    }

    pub fn save_scheduled_config(&self, c: &ScheduledBackup) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO scheduled_backups (guild_id, enabled, frequency, time_of_day, next_run, slot_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(guild_id) DO UPDATE SET
               enabled=excluded.enabled, frequency=excluded.frequency, time_of_day=excluded.time_of_day,
               next_run=excluded.next_run, slot_count=excluded.slot_count",
            params![
                c.guild_id,
                i64::from(c.enabled),
                c.frequency,
                c.time_of_day,
                c.next_run,
                c.slot_count
            ],
        )?;
        Ok(())
    }

    /// Parity: Go `ListScheduledBackups` — only enabled schedules.
    pub fn list_scheduled_backups(&self) -> Result<Vec<ScheduledBackup>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT guild_id, enabled, frequency, time_of_day, next_run, slot_count FROM scheduled_backups WHERE enabled = 1",
        )?;
        let rows = stmt.query_map([], map_scheduled_backup)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

fn map_scheduled_backup(row: &Row<'_>) -> rusqlite::Result<ScheduledBackup> {
    let enabled: i64 = row.get(1)?;
    Ok(ScheduledBackup {
        guild_id: row.get(0)?,
        enabled: enabled != 0,
        frequency: row.get(2)?,
        time_of_day: row.get(3)?,
        next_run: row.get(4)?,
        slot_count: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        let conn = Connection::open_in_memory().expect("in-memory db");
        let store = Store::from_connection(conn);
        store.migrate().expect("migrate");
        store
    }

    /// Seed a `guilds` row: regex/csv tables have FKs → guilds (enforced, parity).
    fn seed_guild(store: &Store, guild_id: &str) {
        store
            .save_guild_config(&GuildConfig {
                guild_id: guild_id.into(),
                verify_channel_id: String::new(),
                domain: "school.cz".into(),
                mode: "REGEX".into(),
                subject: String::new(),
                code_ttl_ns: 600_000_000_000,
                max_attempts: 5,
                rate_limit_count: 3,
                rate_limit_window_ns: 900_000_000_000,
                default_role_id: String::new(),
            })
            .expect("seed guild");
    }

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .expect("prepare");
        let names = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .expect("query_map");
        names.collect::<Result<Vec<_>, _>>().expect("collect")
    }

    fn columns_of(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare pragma");
        let cols = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .expect("query_map");
        cols.collect::<Result<Vec<_>, _>>().expect("collect")
    }

    #[test]
    fn migrate_creates_all_tables() {
        let store = test_store();
        let conn = store.conn();
        let tables = table_names(&conn);
        for expected in [
            "guilds",
            "regex_rules",
            "csv_mappings",
            "csv_emails",
            "verified_users",
            "pending_codes",
            "send_log",
            "user_locales",
            "backups",
            "scheduled_backups",
        ] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table {expected}"
            );
        }
    }

    #[test]
    fn migrate_is_idempotent() {
        let store = test_store();
        store.migrate().expect("second migrate");
        store.migrate().expect("third migrate");
        let conn = store.conn();
        assert_eq!(
            columns_of(&conn, "guilds")
                .iter()
                .filter(|c| c.as_str() == "rate_limit_count")
                .count(),
            1
        );
        assert_eq!(
            columns_of(&conn, "guilds")
                .iter()
                .filter(|c| c.as_str() == "default_role_id")
                .count(),
            1
        );
    }

    #[test]
    fn legacy_rate_limit_per_hour_is_backfilled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("old.db");
        {
            let conn = Connection::open(&path).expect("open legacy db");
            conn.execute_batch(
                "CREATE TABLE guilds (
                    guild_id          TEXT PRIMARY KEY,
                    verify_channel_id TEXT,
                    domain            TEXT,
                    mode              TEXT,
                    subject           TEXT,
                    code_ttl          INTEGER,
                    max_attempts      INTEGER,
                    rate_limit_per_hour INTEGER
                );",
            )
            .expect("create legacy guilds");
            conn.execute(
                "INSERT INTO guilds (guild_id, rate_limit_per_hour) VALUES ('g1', 2)",
                [],
            )
            .expect("insert legacy row");
        }
        let store = Store::open(path.to_str().unwrap()).expect("open runs migrations");
        let cfg = store
            .get_guild_config("g1")
            .unwrap()
            .expect("guild present");
        assert_eq!(cfg.rate_limit_count, 2, "legacy count backfilled");
        assert_eq!(cfg.rate_limit_window_ns, 15, "legacy window set to 15");
        assert_eq!(cfg.default_role_id, "", "default_role_id added");
    }

    #[test]
    fn default_role_column_added_to_old_db() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("old2.db");
        {
            let conn = Connection::open(&path).expect("open legacy db");
            conn.execute_batch(
                "CREATE TABLE guilds (
                    guild_id            TEXT PRIMARY KEY,
                    verify_channel_id   TEXT,
                    domain              TEXT,
                    mode                TEXT,
                    subject             TEXT,
                    code_ttl            INTEGER,
                    max_attempts        INTEGER,
                    rate_limit_count    INTEGER NOT NULL DEFAULT 3,
                    rate_limit_window   INTEGER NOT NULL DEFAULT 15
                );",
            )
            .expect("create guilds without default_role_id");
        }
        let store = Store::open(path.to_str().unwrap()).expect("open runs migrations");
        let conn = store.conn();
        let guilds_cols = columns_of(&conn, "guilds");
        assert!(guilds_cols.contains(&"default_role_id".to_string()));
    }

    #[test]
    fn guild_config_round_trip() {
        let store = test_store();
        let g = GuildConfig {
            guild_id: "100".into(),
            verify_channel_id: "200".into(),
            domain: "sspu-opava.cz".into(),
            mode: "REGEX".into(),
            subject: "Verification".into(),
            code_ttl_ns: 600_000_000_000,
            max_attempts: 5,
            rate_limit_count: 3,
            rate_limit_window_ns: 900_000_000_000,
            default_role_id: "300".into(),
        };
        store.save_guild_config(&g).unwrap();
        let got = store.get_guild_config("100").unwrap().expect("present");
        assert_eq!(got, g);
    }

    #[test]
    fn guild_config_upsert_preserves_row() {
        let store = test_store();
        let mut g = GuildConfig {
            guild_id: "1".into(),
            verify_channel_id: "a".into(),
            domain: "d".into(),
            mode: "REGEX".into(),
            subject: "s".into(),
            code_ttl_ns: 1,
            max_attempts: 5,
            rate_limit_count: 3,
            rate_limit_window_ns: 900_000_000_000,
            default_role_id: "r".into(),
        };
        store.save_guild_config(&g).unwrap();
        g.domain = "changed.cz".into();
        store.save_guild_config(&g).unwrap();
        let got = store.get_guild_config("1").unwrap().unwrap();
        assert_eq!(got.domain, "changed.cz");
        assert_eq!(got.default_role_id, "r");
    }

    #[test]
    fn get_guild_config_missing_returns_none() {
        let store = test_store();
        assert!(store.get_guild_config("nope").unwrap().is_none());
    }

    #[test]
    fn regex_rules_crud_and_priority_order() {
        let store = test_store();
        seed_guild(&store, "g");
        let id_low = store.add_regex_rule("g", ".*@school.cz", "r1", 10).unwrap();
        let id_high = store.add_regex_rule("g", ".*@admin.cz", "r2", 50).unwrap();
        assert!(id_low > 0 && id_high > id_low);
        let rules = store.list_regex_rules("g").unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].id, id_high, "higher priority first");
        assert_eq!(rules[1].id, id_low);

        store.remove_regex_rule(id_low).unwrap();
        assert_eq!(store.list_regex_rules("g").unwrap().len(), 1);
    }

    #[test]
    fn regex_rules_are_guild_scoped() {
        let store = test_store();
        seed_guild(&store, "g1");
        seed_guild(&store, "g2");
        store.add_regex_rule("g1", "a", "r1", 0).unwrap();
        store.add_regex_rule("g2", "b", "r2", 0).unwrap();
        let g1 = store.list_regex_rules("g1").unwrap();
        assert_eq!(g1.len(), 1);
        assert_eq!(g1[0].pattern, "a");
    }

    #[test]
    fn csv_email_mapping_and_join() {
        let store = test_store();
        seed_guild(&store, "g");
        store.insert_csv_email("g", "s1@x.cz", "3A").unwrap();
        store.insert_csv_email("g", "s2@x.cz", "3B").unwrap();
        store.map_csv_class("g", "3A", "roleA").unwrap();
        store.map_csv_class("g", "3B", "roleB").unwrap();

        assert_eq!(
            store
                .get_role_by_csv_email("g", "s1@x.cz")
                .unwrap()
                .unwrap(),
            "roleA"
        );
        assert_eq!(
            store
                .get_role_by_csv_email("g", "s2@x.cz")
                .unwrap()
                .unwrap(),
            "roleB"
        );
        assert!(
            store
                .get_role_by_csv_email("g", "unknown@x.cz")
                .unwrap()
                .is_none()
        );

        store.map_csv_class("g", "3A", "roleA2").unwrap();
        assert_eq!(
            store
                .get_role_by_csv_email("g", "s1@x.cz")
                .unwrap()
                .unwrap(),
            "roleA2"
        );

        store.unmap_csv_class("g", "3B").unwrap();
        assert!(
            store
                .get_role_by_csv_email("g", "s2@x.cz")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn csv_clear_only_affects_guild() {
        let store = test_store();
        seed_guild(&store, "g");
        seed_guild(&store, "other");
        store.insert_csv_email("g", "a@x.cz", "A").unwrap();
        store.insert_csv_email("other", "b@x.cz", "B").unwrap();
        store.clear_csv_emails("g").unwrap();
        assert!(
            store
                .get_role_by_csv_email("g", "a@x.cz")
                .unwrap()
                .is_none()
        );
        store.map_csv_class("other", "B", "rb").unwrap();
        assert_eq!(
            store
                .get_role_by_csv_email("other", "b@x.cz")
                .unwrap()
                .unwrap(),
            "rb"
        );
    }

    #[test]
    fn verified_users_upsert_and_fetch() {
        let store = test_store();
        store.set_verified("g", "u1", "a@x.cz", "roleA").unwrap();
        let v = store
            .get_verified_by_email("g", "a@x.cz")
            .unwrap()
            .expect("present");
        assert_eq!(v.discord_id, "u1");
        assert_eq!(v.role_id, "roleA");
        assert!(v.verified_at > 0);

        // Re-verify same user with a new role → upsert.
        store.set_verified("g", "u1", "a@x.cz", "roleB").unwrap();
        let v = store.get_verified_by_email("g", "a@x.cz").unwrap().unwrap();
        assert_eq!(v.role_id, "roleB");
    }

    #[test]
    fn pending_codes_upsert_increment_delete() {
        let store = test_store();
        let p = PendingCode {
            guild_id: "g".into(),
            discord_id: "u".into(),
            email: "a@x.cz".into(),
            code_hash: "deadbeef".into(),
            expires_at: 1_000,
            attempts: 0,
        };
        store.upsert_pending(&p).unwrap();
        let got = store.get_pending("g", "u").unwrap().expect("present");
        assert_eq!(got.code_hash, "deadbeef");
        assert_eq!(got.expires_at, 1_000);

        let attempts = store.increment_attempts("g", "u").unwrap();
        assert_eq!(attempts, 1);
        let attempts = store.increment_attempts("g", "u").unwrap();
        assert_eq!(attempts, 2);

        store.delete_pending("g", "u").unwrap();
        assert!(store.get_pending("g", "u").unwrap().is_none());
    }

    #[test]
    fn send_log_count_and_prune() {
        let store = test_store();
        let now = 2_000_000;
        // Old send beyond the 2h prune window (inserted first).
        store.log_send("g", "u", now - 10_000).unwrap();
        // Recent sends; the last one prunes rows older than `now - 2h`.
        store.log_send("g", "u", now).unwrap();
        store.log_send("g", "u", now).unwrap();
        store.log_send("g", "u", now).unwrap();
        assert_eq!(store.count_sends_since("g", "u", now - 900).unwrap(), 3);
        assert_eq!(
            store.count_sends_since("g", "u", 0).unwrap(),
            3,
            "old row pruned"
        );
        assert_eq!(store.count_sends_since("other", "u", 0).unwrap(), 0);
    }

    #[test]
    fn user_locales_round_trip() {
        let store = test_store();
        assert!(store.get_user_locale("g", "u").unwrap().is_none());
        store.set_user_locale("g", "u", "cs").unwrap();
        assert_eq!(store.get_user_locale("g", "u").unwrap().unwrap(), "cs");
        store.set_user_locale("g", "u", "en").unwrap();
        assert_eq!(store.get_user_locale("g", "u").unwrap().unwrap(), "en");
    }

    #[test]
    fn backup_records_crud_and_filters() {
        let store = test_store();
        let mk = |guild: &str, kind: &str, created: i64| BackupRecord {
            id: 0,
            guild_id: guild.into(),
            scope: "single".into(),
            kind: kind.into(),
            filepath: format!("/tmp/{guild}_{created}.json"),
            created_at: created,
            channel_count: 1,
            role_count: 2,
            emoji_count: 3,
            ban_count: 4,
        };
        let id1 = store.save_backup(&mk("g", "manual", 100)).unwrap();
        let id2 = store.save_backup(&mk("g", "scheduled", 200)).unwrap();
        let id3 = store.save_backup(&mk("other", "manual", 300)).unwrap();
        assert!(id1 > 0 && id2 > id1 && id3 > id2);

        let all = store.list_backups(None, None).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, id3, "desc by created_at");

        let guild_only = store.list_backups(Some("g"), None).unwrap();
        assert_eq!(guild_only.len(), 2);
        assert_eq!(guild_only[0].id, id2);

        let manual = store.list_backups(Some("g"), Some("manual")).unwrap();
        assert_eq!(manual.len(), 1);
        assert_eq!(manual[0].id, id1);

        let got = store.get_backup(id2).unwrap().expect("present");
        assert_eq!(got.kind, "scheduled");

        store.delete_backup(id2).unwrap();
        assert!(store.get_backup(id2).unwrap().is_none());
    }

    #[test]
    fn scheduled_backups_save_and_enabled_filter() {
        let store = test_store();
        assert!(store.get_scheduled_config("g").unwrap().is_none());

        let sched = ScheduledBackup {
            guild_id: "g".into(),
            enabled: true,
            frequency: "daily".into(),
            time_of_day: "08:00".into(),
            next_run: 1_000,
            slot_count: 3,
        };
        store.save_scheduled_config(&sched).unwrap();
        let got = store.get_scheduled_config("g").unwrap().expect("present");
        assert!(got.enabled);
        assert_eq!(got.frequency, "daily");

        let list = store.list_scheduled_backups().unwrap();
        assert_eq!(list.len(), 1);

        let disabled = ScheduledBackup {
            enabled: false,
            ..sched
        };
        store.save_scheduled_config(&disabled).unwrap();
        assert!(
            store.list_scheduled_backups().unwrap().is_empty(),
            "only enabled listed"
        );
    }
}
