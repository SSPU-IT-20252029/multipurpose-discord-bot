# Lightweight Multi-Guild Verification Bot

A Discord bot for verifying members via email and automatically assigning roles
based on flexible rules. Supports multiple servers, Regex matching, CSV mapping
uploads, bilingual UI (EN/CS), configurable rate limits, and full guild backup.

## Features

- **Multi-Guild** — Each server has independent config, rules, and CSV data.
- **Verification Flow** — Button → Email modal → Code email → Code modal → Role assignment.
- **Regex Mode** — Priority-ordered regex rules matching on the full email address.
- **CSV Mode** — Upload `email,class` CSV and map classes to Discord roles.
- **Bilingual** — English and Czech translations (114 strings each). Users switch with `/language`.
- **Rate Limits** — Per-server email rate limit (`/ratelimit count:<1-3> window:<15-60>`).
- **Backup System** — Capture and restore guild structure (channels, roles, emojis, bans) with configurable scheduled backups and rotation.
- **Debug Mode** — Run with `--debug` for verbose gateway and interaction logging.

## Requirements

- **Rust** 1.85+ (edition 2024)
- **Discord Bot** — Token from the [Developer Portal](https://discord.com/developers/applications). **Server Members Intent** must be enabled. Bot needs `Manage Roles` permission and `bot` + `applications.commands` scopes. Place the bot's role **above** all roles it assigns.
- **Resend** — API key from [resend.com](https://resend.com) and a verified sender domain.

## Quick Start

```bash
# 1. Clone, copy config and env
cp .example.env .env        # fill in DISCORD_TOKEN, RESEND_API_KEY
cp config.example.yml config.yml

# 2. Build and run
cargo run --release         # or `cargo run` for debug build

# 3. With debug logging
cargo run -- --debug
```

## Configuration

Secrets are loaded from `.env`:

```dotenv
DISCORD_TOKEN=your-bot-token
RESEND_API_KEY=re_your_resend_key
# Optional: EMAIL_FROM="Discord bot <discord-bot@yourdomain.com>"
```

Non-secret settings go in `config.yml`:

```yaml
storage:
  dsn: "./data/verifier.db"
  backup_dir: "./backups"
```

## Commands

### Administrator
| Command | Description |
|---|---|
| `/setup domain mode:REGEX\|CSV channel [subject]` | Configure verification and post the Verify button |
| `/regex add pattern role [priority]` | Add a regex rule |
| `/regex list` | List all rules |
| `/regex remove id` | Remove a rule |
| `/csv upload file` | Upload email,class CSV (destructive replace) |
| `/csv map class role` | Map a CSV class to a Discord role |
| `/ratelimit count window` | Max emails per time window (count 1–3, window 15–60 min) |
| `/verifiedrole set role` | Set default role for all verified users |
| `/verifiedrole view` | Show current default role |
| `/verifiedrole clear` | Remove default role |
| `/backup create [scope] [guild-id]` | Create a guild backup |
| `/backup restore id [guild-id]` | Restore from a backup |
| `/backup list [type]` | List backups |
| `/backup schedule frequency [time]` | Enable scheduled backups |
| `/backup schedule-off` | Disable scheduled backups |
| `/backup delete id` | Delete a backup |

### User
| Command | Description |
|---|---|
| `/language en\|cs` | Switch bot language |
| `/help` | Show all commands |

## Verification Workflow

1. Admin runs `/setup` → verify embed with **Verify** button is posted.
2. User clicks **Verify** → enters email in modal.
3. Bot validates domain, pre-checks rules, sends 6-digit code via Resend.
4. User clicks **Enter Code** → enters the 6-digit code.
5. Bot confirms code → resolves role (regex or CSV) → assigns role(s).

## Development

```bash
cargo test                # run all tests
cargo clippy              # lint
cargo fmt                 # format
cargo build --release     # release binary
```

## License

AGPL v3.0