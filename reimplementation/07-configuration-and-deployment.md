# Configuration & Deployment

## Configuration File

The bot uses a single YAML file (default path: `config.yml`, overridable via `-config` flag).

### Format

```yaml
discord:
  token: ${DISCORD_TOKEN}

email:
  api_key: ${RESEND_API_KEY}
  from: "Discord bot <discord-bot@yourdomain.com>"

storage:
  dsn: "./data/verifier.db"
  backup_dir: "./backups"
```

### Environment Variable Substitution

The config loader replaces `${VAR_NAME}` patterns with environment variable values before parsing YAML. This means secrets can be injected via environment variables rather than stored in the file.

### Defaults

- `storage.dsn`: `./data/verifier.db`
- `storage.backup_dir`: `./backups`

### Validation (startup fails if missing)

- `discord.token` — required
- `email.api_key` — required
- `email.from` — required

## Runtime Flags

| Flag | Default | Description |
|---|---|---|
| `-config` | `config.yml` | Path to YAML configuration file |
| `-debug` | false | Enable debug logging (discord gateway + interaction details) |

## Discord Requirements

- **Server Members Intent** must be enabled in Developer Portal
- Bot needs `Manage Roles` permission
- Bot's role must be placed **above** all roles it needs to assign
- OAuth2 scopes: `bot` + `applications.commands`

## Email Provider

The bot uses **Resend** (`resend.com`) for transactional email delivery. An API key (`re_...`) and a verified sender domain are required. The `from` field can include a display name: `"Discord bot <discord-bot@yourdomain.com>"`.

## Deployment

### Docker

A multi-stage Docker build is provided:

1. **Builder stage**: `golang:1.27-alpine` — downloads deps, builds static binary with `CGO_ENABLED=0`
2. **Runtime stage**: `alpine:latest` with `ca-certificates` and `tzdata`
3. Binary runs as `/app/verifier-bot`

### Docker Compose

```yaml
services:
  discord-bot:
    build:
      context: ../../
      dockerfile: deploy/docker/Dockerfile
    volumes:
      - ../../data:/app/data      # SQLite database persistence
      - ../config.yml:/app/config.yml:ro  # Read-only config
    env_file:
      - ../.env                            # Discord token & Resend key
```

### Resource Profile

- Memory: designed to stay under 50 MB RAM
- Storage: SQLite database + JSON backups + emoji assets
- No network ports exposed (Discord gateway is outbound-only)

## Required Environment Variables

| Variable | Source |
|---|---|
| `DISCORD_TOKEN` | Discord Developer Portal |
| `RESEND_API_KEY` | Resend.com dashboard |

## CI/CD

GitHub Actions workflow:
- Trigger: push/PR to `main`
- Steps: checkout, setup Go (version from go.mod), build, test