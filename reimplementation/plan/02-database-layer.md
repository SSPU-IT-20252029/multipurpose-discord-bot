# Session 2 — Database Layer

> **Goal:** `store.rs` — a SQLite layer that is **schema-compatible with the Go store**
> (`internal/store/store.go`), including migrations. All CRUD operations used by every other
> session, plus in-memory unit tests.
>
> **Parity target:** Go `internal/store/store.go` (600 lines) — table DDL, pragmas,
> `migrate()`, `migrateRateLimit()`, `migrateDefaultRole()`.

---

## 2.1 Connection & pragmas

```rust
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,   // single writer, mirrors Go SetMaxOpenConns(1)
}

impl Store {
    pub fn open(dsn: &str) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(dsn)?;
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Self { conn: Arc::new(Mutex::new(conn)) };
        store.migrate()?;
        Ok(store)
    }
}
```

> **Design note.** `rusqlite::Connection` is `!Sync`; wrapping in `Arc<Mutex<>>` restores
> shareability across async tasks. Operations are microsecond-scale so a `std::sync::Mutex`
> guard (not a tokio lock) is the right tool. If contention ever appears, upgrade to a
> dedicated writer task with `mpsc`, but the Go bot proves one writer is plenty.
>
> **Tests** use `Connection::open_in_memory()` through a `Store::from_connection` helper so
> tests never touch disk.

## 2.2 Migration framework

Run on every open, idempotent, in a transaction:

```rust
fn migrate(&self) -> Result<(), rusqlite::Error> {
    let mut conn = self.conn.lock().unwrap();
    let tx = conn.transaction()?;

    tx.execute_batch(SCHEMA)?;          // CREATE TABLE IF NOT EXISTS … (all 10 tables)
    Self::migrate_rate_limit(&tx)?;     // idempotent ALTER TABLE guards
    Self::migrate_default_role(&tx)?;

    tx.commit()?;
    Ok(())
}
```

Helper for idempotency — parity with Go, which runs `ALTER TABLE ADD COLUMN` and **ignores the
"duplicate column name" error** (no introspection for these two):

```rust
fn is_duplicate_column(err: &rusqlite::Error) -> bool {
    matches!(err, rusqlite::Error::SqliteFailure(_, Some(msg))
        if msg.contains("duplicate column name"))
}
```

The legacy `rate_limit_per_hour` detection uses the table-valued `pragma_table_info` (Go parity):

```rust
let has_old: i64 = tx.query_row(
    "SELECT COUNT(*) FROM pragma_table_info('guilds') WHERE name = 'rate_limit_per_hour'",
    [], |row| row.get(0))?;
```

## 2.3 Schema DDL (exact parity — do not rename anything)

```sql
-- guilds: per-guild verification config (one row per guild)
-- NOTE: only rate_limit_count/window/default_role_id have NOT NULL DEFAULT.
--       All other columns are plain nullable TEXT/INTEGER (byte-identical to Go).
CREATE TABLE IF NOT EXISTS guilds (
    guild_id            TEXT PRIMARY KEY,
    verify_channel_id   TEXT,
    domain              TEXT,
    mode                TEXT,
    subject             TEXT,
    code_ttl            INTEGER,
    max_attempts        INTEGER,
    rate_limit_count    INTEGER NOT NULL DEFAULT 3,
    rate_limit_window   INTEGER NOT NULL DEFAULT 15,        -- Go stores nanoseconds
    default_role_id     TEXT NOT NULL DEFAULT ''
);

-- regex_rules: per-guild regex → role rules
CREATE TABLE IF NOT EXISTS regex_rules (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id    TEXT NOT NULL REFERENCES guilds(guild_id) ON DELETE CASCADE,
    pattern     TEXT NOT NULL,
    role_id     TEXT NOT NULL,
    priority    INTEGER NOT NULL DEFAULT 0
);

-- csv_mappings: class → role
CREATE TABLE IF NOT EXISTS csv_mappings (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id    TEXT NOT NULL REFERENCES guilds(guild_id) ON DELETE CASCADE,
    class_name  TEXT NOT NULL,
    role_id     TEXT NOT NULL,
    UNIQUE (guild_id, class_name)
);

-- csv_emails: email → class
CREATE TABLE IF NOT EXISTS csv_emails (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id    TEXT NOT NULL REFERENCES guilds(guild_id) ON DELETE CASCADE,
    email       TEXT NOT NULL,
    class_name  TEXT NOT NULL,
    UNIQUE (guild_id, email)
);

-- verified_users
CREATE TABLE IF NOT EXISTS verified_users (
    guild_id    TEXT NOT NULL,
    discord_id  TEXT NOT NULL,
    email       TEXT NOT NULL,
    role_id     TEXT NOT NULL,
    verified_at INTEGER NOT NULL,
    PRIMARY KEY (guild_id, discord_id),
    UNIQUE (guild_id, email)
);

-- pending_codes
CREATE TABLE IF NOT EXISTS pending_codes (
    guild_id    TEXT NOT NULL,
    discord_id  TEXT NOT NULL,
    email       TEXT NOT NULL,
    code_hash   TEXT NOT NULL,             -- SHA256 hex
    expires_at  INTEGER NOT NULL,          -- unix seconds
    attempts    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (guild_id, discord_id)
);

-- send_log (rate-limit tracking)
CREATE TABLE IF NOT EXISTS send_log (
    guild_id    TEXT NOT NULL,
    discord_id  TEXT NOT NULL,
    sent_at     INTEGER NOT NULL           -- unix seconds
);
CREATE INDEX IF NOT EXISTS idx_send_log_user_time
    ON send_log (guild_id, discord_id, sent_at);

-- user_locales
CREATE TABLE IF NOT EXISTS user_locales (
    guild_id    TEXT NOT NULL,
    user_id     TEXT NOT NULL,
    locale      TEXT NOT NULL DEFAULT 'en',
    PRIMARY KEY (guild_id, user_id)
);

-- backups
CREATE TABLE IF NOT EXISTS backups (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    guild_id      TEXT NOT NULL,
    scope         TEXT NOT NULL DEFAULT 'single',
    kind          TEXT NOT NULL DEFAULT 'manual',
    filepath      TEXT NOT NULL,
    created_at    INTEGER NOT NULL,        -- unix seconds
    channel_count INTEGER NOT NULL DEFAULT 0,
    role_count    INTEGER NOT NULL DEFAULT 0,
    emoji_count   INTEGER NOT NULL DEFAULT 0,
    ban_count     INTEGER NOT NULL DEFAULT 0
);

-- scheduled_backups
CREATE TABLE IF NOT EXISTS scheduled_backups (
    guild_id    TEXT PRIMARY KEY,
    enabled     INTEGER NOT NULL DEFAULT 0,   -- 0 | 1
    frequency   TEXT NOT NULL DEFAULT '',
    time_of_day TEXT NOT NULL DEFAULT '',
    next_run    INTEGER NOT NULL DEFAULT 0,
    slot_count  INTEGER NOT NULL DEFAULT 3
);
```

### Migration steps (idempotent, run after `CREATE TABLE IF NOT EXISTS`)

```rust
fn migrate_rate_limit(tx: &rusqlite::Transaction) -> Result<(), rusqlite::Error> {
    // 1. Idempotent ADD COLUMN — ignore "duplicate column name" (Go parity).
    add_column("ALTER TABLE guilds ADD COLUMN rate_limit_count INTEGER NOT NULL DEFAULT 3", tx)?;
    add_column("ALTER TABLE guilds ADD COLUMN rate_limit_window INTEGER NOT NULL DEFAULT 15", tx)?;

    // 2. Legacy rate_limit_per_hour → count/window (Go's exact SQL + WHERE).
    if has_old_column(tx, "rate_limit_per_hour")? {
        tx.execute_batch(
            "UPDATE guilds SET rate_limit_count = rate_limit_per_hour, rate_limit_window = 15
             WHERE rate_limit_per_hour IS NOT NULL AND rate_limit_count = 3 AND rate_limit_window = 15",
        )?;
    }

    // 3. NULL/zero backfill to defaults (Go parity).
    tx.execute_batch(
        "UPDATE guilds SET rate_limit_count = COALESCE(NULLIF(rate_limit_count, 0), 3),
             rate_limit_window = COALESCE(NULLIF(rate_limit_window, 0), 15)
         WHERE rate_limit_count IS NULL OR rate_limit_window IS NULL",
    )?;
    Ok(())
}
```

> **Corrected parity facts (verified against `internal/store/store.go`):**
> - Go does **not** use `PRAGMA table_info` for the rate-limit/default-role columns — it runs
>   `ALTER TABLE ... ADD COLUMN` and ignores the "duplicate column name" error string.
> - The legacy `rate_limit_per_hour` detection uses `pragma_table_info('guilds')` (table-valued).
> - Legacy backfill sets `rate_limit_window = 15` (a plain integer, not nanoseconds) — mirror
>   exactly; it only affects pre-migration rows.
> - `rate_limit_window` is otherwise stored in **nanoseconds** (`time.Duration`), not minutes.

---

## 2.4 Data model structs

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct GuildConfig {
    pub guild_id: String,
    pub verify_channel_id: String,
    pub domain: String,
    pub mode: Mode,               // Mode::Regex | Mode::Csv (serialized "REGEX"/"CSV")
    pub subject: String,
    pub code_ttl_ns: i64,             // nanoseconds, parity with Go
    pub max_attempts: i64,
    pub rate_limit_count: i64,
    pub rate_limit_window_ns: i64,    // NANOSECONDS (Go stores time.Duration)
    pub default_role_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegexRule { pub id: i64, pub guild_id: String, pub pattern: String, pub role_id: String, pub priority: i64 }

#[derive(Debug, Clone, PartialEq)]
pub struct CsvMapping { pub id: i64, pub guild_id: String, pub class_name: String, pub role_id: String }

#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedUser { pub guild_id: String, pub discord_id: String, pub email: String, pub role_id: String, pub verified_at: i64 }

#[derive(Debug, Clone, PartialEq)]
pub struct PendingCode { pub guild_id: String, pub discord_id: String, pub email: String, pub code_hash: String, pub expires_at: i64, pub attempts: i64 }

#[derive(Debug, Clone, PartialEq)]
pub struct BackupRecord {
    pub id: i64, pub guild_id: String, pub scope: String, pub kind: String,
    pub filepath: String, pub created_at: i64,
    pub channel_count: i64, pub role_count: i64, pub emoji_count: i64, pub ban_count: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledBackup {
    pub guild_id: String, pub enabled: bool, pub frequency: String,
    pub time_of_day: String, pub next_run: i64, pub slot_count: i64,
}
```

`mode` is a plain `String` (`"REGEX"` / `"CSV"`), matching Go's string field exactly.
`CsvMapping` is not exposed as a store struct in Go (only used via the JOIN + upsert), so it is
omitted here.

---

## 2.5 Store API surface (all methods)

### Guild config

```rust
pub fn save_guild_config(&self, cfg: &GuildConfig) -> Result<(), StoreError>;   // UPSERT (full row)
pub fn get_guild_config(&self, guild_id: &str) -> Result<Option<GuildConfig>, StoreError>;
pub fn list_guild_configs(&self) -> Result<Vec<GuildConfig>, StoreError>;
// NOTE: no delete_guild_config in Go — omitted (cascade is handled by other tables).
```

`save_guild_config` uses `INSERT INTO guilds (...) VALUES (...) ON CONFLICT(guild_id) DO UPDATE …`
updating **all** columns including `default_role_id`. The `/setup` handler **must preserve**
`default_role_id` across re-setup (it reads the old value first; preservation happens in the
command layer, not the store).

### Regex rules

```rust
pub fn add_regex_rule(&self, guild_id: &str, pattern: &str, role_id: &str, priority: i64) -> Result<i64, StoreError>; // returns rowid (Go discards it)
pub fn list_regex_rules(&self, guild_id: &str) -> Result<Vec<RegexRule>, StoreError>; // ORDER BY priority DESC
pub fn remove_regex_rule(&self, id: i64) -> Result<(), StoreError>;  // by id only, NOT guild-scoped (Go parity)
```

### CSV data

```rust
pub fn clear_csv_emails(&self, guild_id: &str) -> Result<(), StoreError>;
pub fn insert_csv_email(&self, guild_id: &str, email: &str, class: &str) -> Result<(), StoreError>; // ON CONFLICT(guild_id,email) DO UPDATE
pub fn get_role_by_csv_email(&self, guild_id: &str, email: &str) -> Result<Option<String>, StoreError>;
pub fn map_csv_class(&self, guild_id: &str, class: &str, role_id: &str) -> Result<(), StoreError>; // ON CONFLICT(guild_id,class)
pub fn unmap_csv_class(&self, guild_id: &str, class: &str) -> Result<(), StoreError>;
```

> **FK note:** `foreign_keys=ON` is enforced (parity), so regex/csv **inserts** require an
> existing `guilds` row for that guild. In practice `/setup` runs first. `SELECT`s are unaffected.

`get_csv_role_by_email` is the JOIN from the design doc:

```sql
SELECT m.role_id
  FROM csv_emails e
  JOIN csv_mappings m ON e.guild_id = m.guild_id AND e.class_name = m.class_name
 WHERE e.guild_id = ?1 AND e.email = ?2;
```

### Verified users

```rust
pub fn set_verified(&self, guild_id: &str, discord_id: &str, email: &str, role_id: &str) -> Result<(), StoreError>;
pub fn get_verified_by_email(&self, guild_id: &str, email: &str) -> Result<Option<VerifiedUser>, StoreError>;
// NOTE: no get_verified_by_discord in Go — omitted.
```

`set_verified` uses `INSERT … ON CONFLICT(guild_id, discord_id) DO UPDATE SET email, role_id,
verified_at` (Go parity) and stamps `verified_at = now` internally (Go does `time.Now().Unix()`).

### Pending codes

```rust
pub fn upsert_pending(&self, p: &PendingCode) -> Result<(), StoreError>;   // PK (guild_id, discord_id)
pub fn get_pending(&self, guild_id: &str, discord_id: &str) -> Result<Option<PendingCode>, StoreError>;
pub fn delete_pending(&self, guild_id: &str, discord_id: &str) -> Result<(), StoreError>;
pub fn increment_attempts(&self, guild_id: &str, discord_id: &str) -> Result<i64, StoreError>; // UPDATE … RETURNING attempts
```

### Send log / rate limiting

```rust
pub fn log_send(&self, guild_id: &str, discord_id: &str, at: i64) -> Result<(), StoreError>;
//   INSERT the send THEN DELETE rows older than at - 2h, in the SAME call (Go parity).
pub fn count_sends_since(&self, guild_id: &str, discord_id: &str, since: i64) -> Result<i64, StoreError>;
```

Prune cutoff is `at - 2h` relative to the send being logged (Go: `at.Add(-2*time.Hour)`).
`count_sends_since` uses `WHERE sent_at >= ?` over the composite index.

### User locales

```rust
pub fn get_user_locale(&self, guild_id: &str, user_id: &str) -> Result<Option<String>, StoreError>;
pub fn set_user_locale(&self, guild_id: &str, user_id: &str, locale: &str) -> Result<(), StoreError>; // UPSERT
```

### Backups metadata

```rust
pub fn save_backup(&self, b: &BackupRecord) -> Result<i64, StoreError>;              // returns LastInsertId (Go parity)
pub fn list_backups(&self, guild_id: Option<&str>, kind: Option<&str>) -> Result<Vec<BackupRecord>, StoreError>; // both filters optional, ORDER BY created_at DESC
pub fn get_backup(&self, id: i64) -> Result<Option<BackupRecord>, StoreError>;       // by id only, NOT guild-scoped
pub fn delete_backup(&self, id: i64) -> Result<(), StoreError>;                      // by id only
```

### Scheduled backups

```rust
pub fn save_scheduled_config(&self, s: &ScheduledBackup) -> Result<(), StoreError>; // UPSERT on guild_id
pub fn get_scheduled_config(&self, guild_id: &str) -> Result<Option<ScheduledBackup>, StoreError>;
pub fn list_scheduled_backups(&self) -> Result<Vec<ScheduledBackup>, StoreError>;   // WHERE enabled = 1
```

> `enabled` maps to/from SQLite `0/1`. The scheduler polls `list_scheduled_backups` (enabled
> only) and compares `next_run <= now` in the service layer (Go does the same).

---

## 2.6 Store error

```rust
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")] Db(#[from] rusqlite::Error),
    #[error("i/o error: {0}")] Io(#[from] std::io::Error),
}
```

> Implemented as-is: no `NotFound`/`Constraint` variants — absence is expressed via
> `Result<Option<T>, _>`, and constraints bubble up as `Db` errors (Go does the same).

---

## 2.7 Unit tests (in-memory)

Use `Store::from_connection(Connection::open_in_memory().unwrap())`.

| Area | Test |
|---|---|
| Migrations | `open` on fresh DB creates all 10 tables |
| Migrations idempotency | `migrate` run 2–3× → no errors, no duplicate columns |
| Legacy column | pre-create `guilds` with `rate_limit_per_hour` + no new columns → backfill sets `count=rate_limit_per_hour`, `window=15` |
| `save/get_guild_config` | round-trip all fields; missing → `None`; upsert updates row |
| Regex rules | add returns incrementing ids; `list` priority-desc; `remove` by id; guild-scoped listing |
| CSV JOIN | seed guild + mapping + email → role; missing → `None`; unmap breaks the chain |
| CSV clear/upsert | `clear_csv_emails` empties only that guild; mapping upsert updates role |
| Verified users | `set_verified` upserts; fetch by email |
| Pending codes | upsert overwrites per (guild,user); `increment_attempts` returns new count |
| Send log | `count_sends_since` respects cutoff; `log_send` prunes rows older than `at - 2h` |
| Locales | set → get round-trip; missing → `None` |
| Backups | save returns id; `list_backups` filters by optional guild/kind; get/delete by id |
| Scheduled backups | save/get round-trip; `list_scheduled_backups` only enabled |

> Tests seed a `guilds` row before regex/csv inserts because `foreign_keys=ON` is enforced
> (parity with Go — `/setup` creates the row in practice).

---

## 2.8 Session completion checklist

- [ ] `Store::open` + in-memory `from_connection`
- [ ] Full DDL identical to Go schema
- [ ] `migrate_rate_limit` + `migrate_default_role` idempotent + legacy backfill
- [ ] All CRUD methods above implemented
- [ ] All tests green
- [ ] `cargo clippy -- -D warnings` + `cargo fmt --check` clean
