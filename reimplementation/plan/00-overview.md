# Reimplementation Plan — Overview

> **Status:** Plan (implementation scheduled across 9 sessions)
> **Source of truth:** Go codebase at `/home/adamix/go/multipurpose-discord-bot`
> **Reference design docs:** `reimplementation/01-*.md` … `reimplementation/09-*.md` (copies of the Go project's design documentation)

---

## 1. Goal

Recreate **all shipped functionality** of the Go Discord bot (`sspu-verifier`) as a single-process
Rust application. The Rust rewrite must be:

- **Feature-identical** — every command, modal, button, error path, and edge case behaves the same.
- **Database-compatible** — the same SQLite file can be opened by both implementations without a
  data migration; the schema, column types, constraints, and migration approach are preserved.
- **Idiomatic & modern** — written with the newest stable Rust toolchain (`edition = "2024"`),
  current crate ecosystem, and async-first design (the Go bot is single-threaded; we use `tokio`).

### In scope

| Area | Notes |
|---|---|
| Email verification flow (two-modal) | `start()` + `confirm()`, 6-digit codes, SHA256 hashing |
| Role resolution (REGEX + CSV modes) | priority-ordered patterns / class→role JOIN |
| Rate limiting | per-guild `count` / `window`, backed by `send_log` |
| Admin commands | `/setup`, `/regex`, `/csv`, `/ratelimit`, `/verifiedrole` |
| User commands | `/language`, `/help` |
| Guild backup system | capture, restore, JSON, scheduler, rotation, `/backup` group |
| i18n | English + Czech, ~140 strings, per-guild user preference |
| Config & deployment | YAML + `${ENV_VAR}` substitution, Docker, docker-compose, CI |

### Out of scope (documented, not implemented)

Music player, temporary voice channels, moderation suite, ticket system, server statistics,
code compiler, free-game deals. These are recorded in the original `docs/TODO.md` and
`reimplementation/09-planned-features.md` as aspirational.

---

## 2. Crate Selection

All crates chosen for maturity + active maintenance. `Cargo.toml` uses the newest feature sets
available; nothing requires nightly.

| Concern | Crate | Version | Why |
|---|---|---|---|
| Async runtime | `tokio` | 1.x (`full`) | De-facto async runtime; powers serenity + reqwest |
| Discord API | `serenity` | 0.12 | Primary gateway/REST library, very active |
| Command framework | `poise` | 0.6 | Ergonomic slash commands, modals, components, per-guild context |
| SQLite | `rusqlite` | 0.3x (`bundled`) | Statically builds SQLite → no system lib, matches Go's CGO-free goal |
| HTTP client | `reqwest` | 0.12 (`json`, `rustls-tls`) | Resend API, Discord CDN downloads, pure-Rust TLS |
| Serialization | `serde` + `serde_json` | 1.x | Backup JSON, Resend payloads |
| Config YAML | `serde_yaml` | 0.9 | Mirrors `yaml.v3`; `serde_yml` fork is a fallback if needed |
| CLI flags | `clap` | 4.x (`derive`) | `-config`, `-debug` parity |
| Hashing | `sha2` | 0.10 | SHA256 for codes |
| Crypto RNG | `rand` | 0.9 | `random_range(0..1_000_000)` over OS entropy for codes |
| Constant-time compare | `subtle` | 2.x | `ConstantTimeEq` parity with Go's `subtle.ConstantTimeCompare` |
| Regex | `regex` | 1.x | RE2-like syntax, close to Go's `regexp` |
| CSV | `csv` | 1.x | Parser for `/csv upload` |
| Time | `chrono` | 0.4 | Backup filenames `YYYYMMDD_HHMMSS`, scheduling |
| Logging | `tracing` + `tracing-subscriber` | 0.1 / 0.3 | Structured logging, `-debug` toggles `poise` verbosity |
| Templating | inline `format!` | — | Email templates are small; avoid a template engine dependency |

---

## 3. Dependency Graph

```
crate root (lib.rs)
  │
  ├── config       ── serde_yaml, regex (${ENV} expansion)
  ├── store        ── rusqlite
  │    ▲
  │    │ (used by verify, backup, handlers)
  ├── mailer       ── reqwest (Resend REST)
  │    ▲
  │    │ (used by verify)
  ├── verify       ── rand, sha2, subtle, store, mailer, i18n
  │    ▲
  │    │ (used by interaction handlers)
  ├── backup       ── reqwest (Discord CDN + REST), chrono, store
  │    ▲
  │    │ (used by /backup group + scheduler task)
  ├── i18n         ── no deps (static maps)
  └── bot          ── poise/serenity, all of the above (command handlers)
```

---

## 4. Target Project Structure

```
multipurpose-discord-bot/
├── Cargo.toml
├── .env.example
├── .gitignore
├── config.example.yml
├── Dockerfile
├── docker-compose.yml
├── .github/workflows/ci.yml
├── src/
│   ├── main.rs            ← tokio entry point, wires everything together
│   ├── lib.rs             ← module tree + shared public API
│   ├── config.rs          ← config loading, env expansion, validation
│   ├── error.rs           ← unified Error enum
│   ├── store.rs           ← SQLite layer: schema, migrations, CRUD
│   ├── verify.rs          ← verification service + role resolution + rate limit
│   ├── mailer.rs          ← Resend client
│   ├── i18n.rs            ← Translations struct, EN/CS maps, locale parsing
│   ├── backup.rs          ← capture/restore/json/scheduler (may split into mod dir)
│   └── bot/
│       ├── mod.rs         ← Bot state, command registration, dispatch
│       ├── commands.rs    ← slash command handlers
│       ├── components.rs  ← button + modal handlers (verify flow)
│       └── context.rs     ← poise Context wrapper + helpers (locale, respond)
└── tests/
    ├── store_integration.rs
    ├── verify_integration.rs
    └── mailer_mock.rs
```

> `backup.rs` may become `src/backup/{mod,capture,restore,json_io,scheduler}.rs` once it grows;
> mirror the Go `internal/backup/` layout.

---

## 5. Session Roadmap

Each session is self-contained: it leaves the project in a compilable, tested state.

| # | Session | Deliverables | Tests |
|---|---|---|---|
| 1 | **Project scaffold & config** | `Cargo.toml`, `main.rs`, `config.rs`, `error.rs`, logging | config expansion + validation |
| 2 | **Database layer** | `store.rs`: full schema, migrations, all CRUD | in-memory store tests |
| 3 | **i18n + mailer** | `i18n.rs`, `mailer.rs`, email templates | locale parsing, mailer via mock HTTP |
| 4 | **Verification service** | `verify.rs`: start/confirm, role resolution, rate limit | unit + mocked-store tests |
| 5 | **Discord core** | Bot struct, `on_ready`, global registration, dispatch, `/help` | — (requires Discord token) |
| 6 | **Admin commands I** | `/setup`, `/regex`, `/csv`, verify button + email modal | — |
| 7 | **Admin commands II + user flow** | `/ratelimit`, `/verifiedrole`, `/language`, code modal, full flow | — |
| 8 | **Backup system** | capture, emoji download, restore, JSON, scheduler, `/backup` group | backup JSON round-trip |
| 9 | **Deploy & polish** | Dockerfile, compose, CI, `.env.example`, release build, docs | full `cargo test` + clippy |

---

## 6. Database Compatibility Guarantee

The single most important constraint of the rewrite. The Rust store must produce **byte-compatible
schema behavior** with the Go store:

1. **Identical DDL** — same table names, column names, column types (`TEXT`/`INTEGER`), primary
   keys, foreign keys (with `CASCADE`), `UNIQUE` constraints, and index names
   (`idx_send_log_user_time`).
2. **Identical defaults** — e.g. `rate_limit_count = 3`, `rate_limit_window = 15`,
   `default_role_id = ''`.
3. **Identical migration strategy** — `CREATE TABLE IF NOT EXISTS` on startup + idempotent
   `ALTER TABLE ADD COLUMN` guards for `rate_limit_count`, `rate_limit_window`,
   `default_role_id`, plus legacy `rate_limit_per_hour` column migration.
4. **Identical pragmas** — `busy_timeout=5000`, `journal_mode=WAL`, `foreign_keys=1`, single
   writer connection.
5. **Time semantics** — unix seconds (`INTEGER`) everywhere `created_at`/`expires_at`/`sent_at`
   are used. `code_ttl` in the Go DB is nanoseconds (`INTEGER`) — preserved as-is.

Verification: open a database produced by the Go bot with the Rust binary; run the full command
surface; then open the same file with the Go bot again and confirm no schema drift.

---

## 7. Cross-Cutting Conventions

- **IDs as strings** — Discord IDs (`guild_id`, `discord_id`, `role_id`, `channel_id`) are
  `TEXT`, mirrored as `String` (newtype) in Rust. Avoid `u64` in the DB layer.
- **Ephemeral responses** — every verification interaction replies ephemeral; no verification
  data ever posts to a shared channel.
- **Global commands** — all slash commands registered globally via bulk overwrite on `on_ready`.
- **Admin gating** — admin commands require `Administrator` via `default_member_permissions`.
- **Locale-first responses** — every user-facing string resolves through `i18n::get(locale)`
  using the per-guild user preference.

---

## 8. Definition of Done

A session is **done** when:

1. Code compiles with `cargo build` (and `cargo build --release` in Session 9).
2. `cargo clippy -- -D warnings` is clean.
3. `cargo fmt --check` is clean.
4. New unit tests pass: `cargo test`.
5. Any behavior divergence from the Go bot is documented inline with a `// PARITY:` comment.
6. No secrets committed; `.env`/`config.yml` gitignored.
