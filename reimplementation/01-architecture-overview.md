# Architecture Overview

## What It Is

A lightweight, multi-guild Discord verification bot that authenticates users via school email and automatically assigns Discord roles. It runs as a single stateless HTTP-less process and uses SQLite for persistence.

## High-Level Structure

```
cmd/bot/main.go          ← Entry point, Discord gateway, command routing
internal/
  config/config.go       ← YAML config loader with env var substitution
  store/store.go         ← SQLite database layer (all CRUD)
  verify/service.go      ← Email verification business logic
  mailer/mailer.go       ← Resend API email sender
  i18n/i18n.go           ← Bilingual translation system (EN/CS)
  backup/
    capture.go           ← Discord API → snapshot structs
    types.go             ← Backup data structures
    json_io.go           ← JSON file read/write
    scheduler.go         ← Periodic backup loop
    restore.go           ← Snapshot structs → Discord API
```

## Dependency Graph

```
main.go
  → config (YAML parsing, env var expansion)
  → store (SQLite via modernc.org/sqlite — pure Go, no CGO)
     → used by: verify, backup, main
  → mailer (Resend HTTP API)
     → used by: verify
  → verify (validation, code gen, rate limiting)
     → used by: main (slash command handlers & modals)
  → backup (guild structure capture/restore/schedule)
     → used by: main (slash command handlers & scheduler loop)
  → i18n (static translations)
     → used by: verify, mailer, main
```

## Lifecycle

1. Load `config.yml` with `${ENV_VAR}` substitution
2. Open SQLite database (auto-migrate schema on connect)
3. Create Discord session with intents: `Guilds`, `GuildMembers`, `GuildModeration`, `GuildEmojis`, `GuildBans`
4. Register all slash commands globally via `ApplicationCommandBulkOverwrite`
5. Start backup scheduler goroutine (checks every 60s)
6. Block on `os.Signal` until SIGINT/SIGTERM

## Key Design Decisions

- **Single-file executable** — no external runtime dependencies
- **SQLite with WAL mode** — one concurrent writer (`SetMaxOpenConns(1)`)
- **No web server** — everything happens through Discord gateway interactions
- **Ephemeral responses** — verification flow is private to each user
- **Multi-guild by design** — every table has a `guild_id` column; guilds are isolated
- **Pure Go SQLite** (`modernc.org/sqlite`) — enables CGO_ENABLED=0 Docker builds