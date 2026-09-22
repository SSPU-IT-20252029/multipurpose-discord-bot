# Lightweight Multi-Guild Verification Bot

A Discord bot for verifying members via email and automatically assigning roles based on flexible rules. It supports multiple servers (multi-guild), Regex matching, CSV mapping uploads, bilingual UI (EN/CS), and configurable rate limits.

## Features

- **Multi-Guild**: Each server has its own configuration, rules, and CSV data.
- **Verification Flow**: Button → Modal (email) → Email code → Modal (code) → Role assignment.
- **Regex Mode**: Multiple regex rules per guild, evaluated in order of priority.
- **CSV Mode**: Upload `.csv` with `email,class` columns and map classes to roles.
- **Bilingual**: Built-in English and Czech translations. Users can switch language with `/language`.
- **Rate Limits**: Configurable per-server email rate limits (`/ratelimit`).
- **Debug Mode**: Run with `-debug` for verbose gateway and interaction logging.

## Requirements

- **Go** 1.27+
- **Discord Bot** — A token from the [Developer Portal](https://discord.com/developers/applications), with the **SERVER MEMBERS INTENT** enabled. Use an invite with `Manage Roles` permission and `bot` + `applications.commands` scopes. The bot's role must be placed **above** all the roles it needs to assign.
- **Resend** — An API key from [resend.com](https://resend.com) and a verified sender domain.

## Configuration (Global)

Copy `deploy/config.example.yml` to `deploy/config.yml` and fill it out:

```yaml
discord:
  token: ${DISCORD_TOKEN}

email:
  api_key: ${RESEND_API_KEY}
  from: "Discord bot <discord-bot@yourdomain.com>"

storage:
  dsn: "./data/verifier.db"
```

## Commands

### Administrator
- `/setup` - Initializes the server, sets the allowed email domain, mode (REGEX or CSV), and generates a "Verify" button in the chosen channel.
- `/regex add/remove/list` - Manage regex rules.
- `/csv upload` - Uploads a `.csv` file with `email` and `class` columns.
- `/csv map` - Maps a specific `class` from the CSV to a Discord role.
- `/ratelimit count:<1-3> window:<1-60>` - Sets the maximum number of verification emails per time window (in minutes). Default: 3 emails / 15 minutes.
- `/verifiedrole set/view/clear` - Manage the default role assigned to every verified user.

### User
- `/language <en|cs>` - Switch bot language (English or Czech).
- `/help` - Shows an overview of all available commands.

## Workflow

1. A user clicks the "Verify" button (created via `/setup`).
2. A Modal window pops up, prompting the user for their email.
3. The bot checks the configured domain, generates a code, and sends it via email.
4. The bot sends an ephemeral message with an "Enter Code" button to the user.
5. The user enters the code into a second Modal.
6. The bot finds the matching role using either the configured **Regex** or **CSV Mapping** and assigns it. (It will also assign the default verified role if one is configured via `/verifiedrole`).

## Build and Run (Docker / Local)

Running locally:
```bash
go build ./cmd/bot
./bot -config deploy/config.yml
```

With debug logging:
```bash
./bot -config deploy/config.yml -debug
```

The bot is designed to run in a single Docker container with the database (`.db` file) mounted in a volume (`/data`). The memory footprint is optimized to stay under 50 MB RAM.

To run via Docker Compose:
1. Copy `deploy/.example.env` to `deploy/.env` and add your `DISCORD_TOKEN` and `RESEND_API_KEY`.
2. Run `cd deploy/docker && docker-compose up -d --build`.

## License

AGPL v3.0
