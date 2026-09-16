# Guild Backup System

## Overview

The backup system captures Discord guild structure as JSON snapshots, stores them on disk, and can restore them to any guild the bot is in. It supports manual and scheduled backups with automatic rotation.

## Backup Data Structure

```json
{
  "scope": "single" | "multi",
  "guild_id": "...",
  "guild": {
    "name": "...",
    "icon_url": "...",
    "banner_url": "...",
    "verification_level": 0,
    "explicit_content_filter": 0,
    "afk_channel_id": "...",
    "afk_timeout": 300,
    "default_message_notifications": 0,
    "system_channel_id": "...",
    "system_channel_flags": 0,
    "preferred_locale": "en-US"
  },
  "categories": [ /* ChannelEntry */ ],
  "channels": [ /* ChannelEntry */ ],
  "roles": [ /* RoleEntry */ ],
  "emojis": [ /* EmojiEntry with local asset path */ ],
  "bans": [ /* BanEntry */ ],
  "created_at": "2024-01-01T00:00:00Z"
}
```

### ChannelEntry

- id, guild_id, parent_id, name, type, position, topic, nsfw, bitrate, rate_limit_per_user, permission_overwrites `[{id, type, allow, deny}]`

### RoleEntry

- id, guild_id, name, permissions, color, position, hoist, mentionable

### EmojiEntry

- id, name, animated, data_url (local relative path after download)

### BanEntry

- user_id, reason

## Capture Process

1. Fetch guild info, channels, roles, emojis, bans via Discord REST API
2. Separate channels into `categories` (type=GuildCategory) and `channels` (everything else)
3. For each emoji, construct CDN URL and optionally download the image file to `assets/emoji_{id}_{name}.{png|gif}`
4. Store the relative asset path back into the emoji entry's `DataURL`
5. Write JSON to `backups/backup_{guildid}_{timestamp}.json`

## Restore Process

1. Read backup JSON from disk
2. Fetch current channels and roles in the target guild
3. Create categories (skip if name already exists)
4. Create channels with permission overwrites (skip if name + type match)
5. Create roles (skip if case-insensitive name match; skip @everyone)
6. Apply bans (ignore errors — ban may already exist)

## Backup Storage

### On Disk
- Default directory: `./backups/`
- Filename format: `backup_{guildid}_{YYYYMMDD_HHmmss}.json`
- Emoji assets: `assets/emoji_{id}_{name}.{png|gif}`

### Database Records

`backups` table stores metadata:
- id, guild_id, scope, kind (manual/scheduled), filepath, created_at, channel_count, role_count, emoji_count, ban_count

## Scheduled Backups

### Scheduling

- Runs as a goroutine with a 60-second ticker
- On each tick, queries `scheduled_backups` WHERE `enabled = 1 AND next_run <= now`
- For each due guild, launches a goroutine to capture and save

### Frequency Options

| Value | Interval |
|---|---|
| `12h` | 12 hours |
| `daily` | 24 hours |
| `weekly` | 7 days |
| `biweekly` | 14 days |
| `monthly` | 30 days |
| `3months` | 90 days |
| `6months` | 180 days |

### Time-of-Day

- Only applies to `daily` and `weekly` frequencies
- Format: `HH:MM` in UTC
- If specified, the next run is set to today at that time (or tomorrow if already past)

### Rotation

- Only the 3 most recent scheduled backups per guild are kept
- Older scheduled backups are deleted (file + database record)

## Commands

| Command | Description |
|---|---|
| `/backup create [scope:single|multi] [guild-id:<id>]` | Manual backup |
| `/backup restore id:<int> [guild-id:<id>]` | Restore from backup |
| `/backup list [type:all|manual|scheduled]` | List backups |
| `/backup schedule frequency:<freq> [time:<HH:MM>]` | Enable scheduled backups |
| `/backup schedule-off` | Disable scheduled backups |
| `/backup delete id:<int>` | Delete a backup |