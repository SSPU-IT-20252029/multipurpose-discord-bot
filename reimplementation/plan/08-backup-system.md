# Session 8 — Backup System

> **Goal:** `backup.rs` — capture, restore, JSON persistence, scheduled backups with rotation,
> and the `/backup` command group.
>
> **Parity target:** Go `internal/backup/{capture,types,json_io,restore,scheduler}.go`.

---

## 8.1 Module layout

```
src/backup/
├── mod.rs        ← BackupService, public API, command wiring glue
├── types.rs      ← snapshot structs (serde Serialize/Deserialize)
├── capture.rs    ← Discord REST → snapshot
├── restore.rs    ← snapshot → Discord REST
├── json_io.rs    ← read/write JSON files
└── scheduler.rs  ← periodic task + rotation
```

## 8.2 Snapshot types (`types.rs`)

Parity with the JSON structure in the design doc (`reimplementation/04-guild-backup-system.md`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backup {
    pub scope: String,               // "single" | "multi"
    pub guild_id: String,
    pub guild: GuildInfo,
    pub categories: Vec<ChannelEntry>,
    pub channels: Vec<ChannelEntry>,
    pub roles: Vec<RoleEntry>,
    pub emojis: Vec<EmojiEntry>,
    pub bans: Vec<BanEntry>,
    pub created_at: String,          // RFC3339, parity with Go time.Now().Format(time.RFC3339)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildInfo {
    pub name: String,
    pub icon_url: Option<String>,
    pub banner_url: Option<String>,
    pub verification_level: u8,
    pub explicit_content_filter: u8,
    pub afk_channel_id: Option<String>,
    pub afk_timeout: u32,
    pub default_message_notifications: u8,
    pub system_channel_id: Option<String>,
    pub system_channel_flags: u8,
    pub preferred_locale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelEntry {
    pub id: String,
    pub guild_id: Option<String>,
    pub parent_id: Option<String>,
    pub name: String,
    pub channel_type: u8,               // serde rename "type"
    pub position: i32,
    pub topic: Option<String>,
    pub nsfw: bool,
    pub bitrate: Option<u32>,
    pub rate_limit_per_user: Option<u32>,
    pub permission_overwrites: Vec<PermissionOverwrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionOverwrite {
    pub id: String,
    pub kind: u8,                       // serde rename "type": role=0 / member=1
    pub allow: String,                  // bigint as string (parity with Go int64 serialization)
    pub deny: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleEntry {
    pub id: String,
    pub guild_id: Option<String>,
    pub name: String,
    pub permissions: String,            // bigint as string
    pub color: u32,
    pub position: i32,
    pub hoist: bool,
    pub mentionable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmojiEntry {
    pub id: Option<String>,
    pub name: String,
    pub animated: bool,
    pub data_url: Option<String>,       // local relative path after download
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanEntry {
    pub user_id: String,
    pub reason: Option<String>,
}
```

> **Parity note:** verify the **exact** JSON field names the Go bot writes (serde `rename` where
> Go used different casing, e.g. `channel_type` vs `type`, `kind` vs `type` on overwrites). Load
> a real Go-generated backup file and round-trip it through the Rust structs — this is the
> acceptance test.

## 8.3 Capture (`capture.rs`)

```rust
impl BackupService {
    pub async fn capture(&self, http: &Http, guild_id: &str) -> Result<Backup, BackupError> {
        // 1. Guild struct + channels + roles + emojis + bans (serenity model methods).
        let guild = http.get_guild(guild_id).await?;

        // 2. Split channels: type == ChannelType::Category → categories; else channels.
        //    (Parity: Go stored parent_id only on non-category channels.)

        // 3. Emoji download:
        //    for each emoji, build CDN url (cdn_url(guild_emoji) → <name>.<png|gif>)
        //    download via reqwest into assets/emoji_{id}_{name}.{png|gif}
        //    store the *relative* path into emoji.data_url.

        // 4. Bans: user_id + reason.

        // 5. created_at = chrono::Utc::now().to_rfc3339().
    }
}
```

Emoji filename helper:

```rust
fn emoji_filename(e: &EmojiEntry) -> String {
    format!("assets/emoji_{}_{}.{}", e.id, e.name, if e.animated { "gif" } else { "png" })
}
```

## 8.4 JSON I/O (`json_io.rs`)

```rust
pub fn write_backup(dir: &Path, guild_id: &str, b: &Backup) -> Result<String, BackupError>;
// filename: backup_{guildid}_{YYYYMMDD_HHmmss}.json  (manual)
//           scheduled_{guildid}_{YYYYMMDD_HHmmss}.json (scheduled — confirm Go naming)

pub fn read_backup(path: &Path) -> Result<Backup, BackupError>;
```

Serialization: pretty-printed (`serde_json::to_string_pretty`), parity with Go `json.MarshalIndent`.

## 8.5 Restore (`restore.rs`)

```rust
pub async fn restore(&self, http: &Http, target_guild_id: &str, b: &Backup) -> Result<RestoreStats, BackupError> {
    // Fetch current channels + roles of target guild.

    // 1. Create categories that don't already exist (by name).
    // 2. Create channels (skip if a channel with same name + type exists); apply
    //    permission overwrites, topic, nsfw, bitrate, rate_limit, position.
    // 3. Create roles (skip case-insensitive duplicate names; skip "@everyone");
    //    apply permissions/color/hoist/mentionable/position.
    // 4. Apply bans — ignore individual errors (may already exist).

    // Returns counts for the reply: channels/roles/bans created.
}
```

> **Parity:** Go ordered create categories → channels → roles → bans. Any rollback/partial
> semantics documented in Go must be mirrored; ignore-errors on bans is intentional.

## 8.6 Scheduler (`scheduler.rs`)

```rust
pub fn spawn_scheduler(store: Store, backup: BackupService, http: Http) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));  // parity: 60s tick
        loop {
            ticker.tick().await;
            let due = match store.list_due_scheduled_backups(now_unix_secs()) {
                Ok(d) => d, Err(e) => { tracing::error!(%e, "scheduler query"); continue; }
            };
            for sched in due {
                // spawn per-guild task (parity: Go launched a goroutine per due guild)
                let store = store.clone(); let backup = backup.clone(); let http = http.clone();
                tokio::spawn(async move { run_scheduled(&store, &backup, &http, &sched).await });
            }
        }
    })
}
```

Frequency → interval mapping (seconds), parity table:

| Key | Interval |
|---|---|
| `12h` | 43_200 |
| `daily` | 86_400 |
| `weekly` | 604_800 |
| `biweekly` | 1_209_600 |
| `monthly` | 2_592_000 |
| `3months` | 7_776_000 |
| `6months` | 15_552_000 |

Time-of-day (`HH:MM`, UTC) applies **only** to `daily`/`weekly`; `next_run` = today at that time
(or tomorrow if already passed). Otherwise `next_run = now + interval`.

After a successful scheduled capture: update `next_run`, then **rotation**:

```rust
fn rotate_scheduled(store: &Store, guild_id: &str, max: i64) -> Result<(), BackupError> {
    // slot_count = 3 (parity: always 3)
    // list scheduled backups DESC by created_at; delete (file + DB record) all beyond the newest 3.
}
```

## 8.7 `/backup` command group

```rust
#[poise::command(slash_command,
    subcommands("create", "restore", "list", "schedule", "schedule_off", "delete"),
    default_member_permissions = "ADMINISTRATOR")]
async fn backup(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> { Ok(()) }
```

| Subcommand | Options | Behavior |
|---|---|---|
| `create` | `scope: String` (opt, single/multi), `guild_id: String` (opt) | target = provided guild else current; capture + save record (`kind="manual"`); ephemeral summary |
| `restore` | `id: i64` (req), `guild_id: String` (opt) | load record + JSON; restore to target; ephemeral stats |
| `list` | `kind: String` (opt: all/manual/scheduled) | embed of backups (id, date, counts) |
| `schedule` | `frequency: String` (req), `time: String` (opt `HH:MM`) | upsert `scheduled_backups`, enabled=1, compute next_run; ephemeral ack |
| `schedule_off` | — | enabled=0; ephemeral ack |
| `delete` | `id: i64` (req) | delete file + DB record |

`scope == "multi"` is informational metadata stored on the record (Go stores it but does not
change capture logic) — mirror exactly.

> `guild-id` option allows admins to back up a **different** guild the bot belongs to
> (permission-validated by Discord API). Keep the HTTP error path user-friendly and localized.

---

## 8.8 Unit tests (no network — use fixture JSON + mocked capture via trait)

To test capture/restore without a live Discord, extract a `Capturable`/`Restorable` trait
behind the service, or run a `httpmock` server that answers the guild/channels/roles/emojis/bans
endpoints. Recommended tests:

| Test | Covers |
|---|---|
| `write_backup` → `read_backup` round-trip | JSON serialization fidelity (pretty, fields, bigint strings) |
| load a **fixture** from the Go bot and deserialize | schema/field-name parity (acceptance) |
| restore skips existing category by name | restore logic, no dupes |
| restore skips `@everyone` and case-insensitive role dupes | restore edge cases |
| `rotate_scheduled` keeps newest 3 | rotation + file deletion |
| `next_run` computation for `daily` with time-of-day (past/future) | scheduling math |
| `next_run` for `weekly`/`monthly` (interval only) | scheduling math |
| emoji filename for animated vs static | filename parity |

---

## 8.9 Session completion checklist

- [ ] Snapshot types round-trip a real Go-generated backup file
- [ ] Capture downloads emojis, stores relative paths
- [ ] Restore creates categories/channels/roles/bans with skip rules
- [ ] JSON files written with Go-compatible names + pretty format
- [ ] Scheduler ticks every 60s, handles all 7 frequencies, time-of-day for daily/weekly
- [ ] Rotation keeps newest 3 scheduled backups
- [ ] `/backup` group complete with all 6 subcommands + localized replies
- [ ] clippy/fmt clean
