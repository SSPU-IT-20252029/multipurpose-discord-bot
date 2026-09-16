# Database Schema

## Engine

SQLite via `modernc.org/sqlite` (pure Go implementation, no CGO required).

Pragma settings on connection:
- `busy_timeout=5000`
- `journal_mode=WAL`
- `foreign_keys=1`

Max open connections: 1 (single writer).

## Tables

### `guilds`

Per-guild verification configuration. One row per guild.

| Column | Type | Default | Description |
|---|---|---|---|
| `guild_id` | TEXT PK | | Discord guild ID |
| `verify_channel_id` | TEXT | | Channel where the verify button is posted |
| `domain` | TEXT | | Allowed email domain (e.g. `sspu-opava.cz`) |
| `mode` | TEXT | | `REGEX` or `CSV` |
| `subject` | TEXT | | Email subject line |
| `code_ttl` | INTEGER | | Code validity in nanoseconds (default 10 min) |
| `max_attempts` | INTEGER | | Max wrong code attempts (default 5) |
| `rate_limit_count` | INTEGER | 3 | Max email sends per window |
| `rate_limit_window` | INTEGER | 15 | Window in minutes |
| `default_role_id` | TEXT | '' | Role assigned to all verified users |

### `regex_rules`

Per-guild regex patterns mapping emails to roles.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | |
| `guild_id` | TEXT FK → guilds | CASCADE delete |
| `pattern` | TEXT | Go regex pattern |
| `role_id` | TEXT | Discord role ID |
| `priority` | INTEGER | Higher = evaluated first |

### `csv_mappings`

Maps class names (from CSV) to Discord roles.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | |
| `guild_id` | TEXT FK → guilds | CASCADE delete |
| `class_name` | TEXT | Class name from CSV |
| `role_id` | TEXT | Discord role ID |
| UNIQUE(`guild_id`, `class_name`) | | |

### `csv_emails`

Individual email-to-class mappings from uploaded CSV.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | |
| `guild_id` | TEXT FK → guilds | CASCADE delete |
| `email` | TEXT | Full email address |
| `class_name` | TEXT | Class name |
| UNIQUE(`guild_id`, `email`) | | |

### `verified_users`

Tracks which Discord users have verified which emails.

| Column | Type | Description |
|---|---|---|
| `guild_id` | TEXT | |
| `discord_id` | TEXT | |
| `email` | TEXT | |
| `role_id` | TEXT | Assigned role |
| `verified_at` | INTEGER | Unix timestamp |
| PRIMARY KEY(`guild_id`, `discord_id`) | | |
| UNIQUE(`guild_id`, `email`) | | |

### `pending_codes`

Active verification codes awaiting confirmation.

| Column | Type | Description |
|---|---|---|
| `guild_id` | TEXT | |
| `discord_id` | TEXT | |
| `email` | TEXT | |
| `code_hash` | TEXT | SHA256 hex of the code |
| `expires_at` | INTEGER | Unix timestamp |
| `attempts` | INTEGER | Wrong attempts so far |
| PRIMARY KEY(`guild_id`, `discord_id`) | | |

### `send_log`

Rate limit tracking for email sends.

| Column | Type | Description |
|---|---|---|
| `guild_id` | TEXT | |
| `discord_id` | TEXT | |
| `sent_at` | INTEGER | Unix timestamp |

Index: `idx_send_log_user_time` on (`guild_id`, `discord_id`, `sent_at`)

### `user_locales`

User language preferences (per guild).

| Column | Type | Description |
|---|---|---|
| `guild_id` | TEXT | |
| `user_id` | TEXT | |
| `locale` | TEXT | `en` or `cs` |
| PRIMARY KEY(`guild_id`, `user_id`) | | |

### `backups`

Backup record metadata.

| Column | Type | Description |
|---|---|---|
| `id` | INTEGER PK AUTOINCREMENT | |
| `guild_id` | TEXT | |
| `scope` | TEXT | `single` or `multi` |
| `kind` | TEXT | `manual` or `scheduled` |
| `filepath` | TEXT | Path to JSON backup file |
| `created_at` | INTEGER | Unix timestamp |
| `channel_count` | INTEGER | |
| `role_count` | INTEGER | |
| `emoji_count` | INTEGER | |
| `ban_count` | INTEGER | |

### `scheduled_backups`

Scheduled backup configuration (one row per guild).

| Column | Type | Description |
|---|---|---|
| `guild_id` | TEXT PK | |
| `enabled` | INTEGER | 0 or 1 |
| `frequency` | TEXT | `12h`, `daily`, `weekly`, etc. |
| `time_of_day` | TEXT | HH:MM in UTC |
| `next_run` | INTEGER | Unix timestamp |
| `slot_count` | INTEGER | Always 3 |

## Schema Evolution

The bot's `Open()` function runs migrations on every startup:
1. Creates all tables with `CREATE TABLE IF NOT EXISTS`
2. Runs `ALTER TABLE` migrations for columns added after initial release (`rate_limit_count`, `rate_limit_window`, `default_role_id`)
3. Handles migration from an old `rate_limit_per_hour` column to the new `rate_limit_count` + `rate_limit_window` scheme