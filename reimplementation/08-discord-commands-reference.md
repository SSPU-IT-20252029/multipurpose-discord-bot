# Discord Commands Reference

All commands are slash commands registered globally via `ApplicationCommandBulkOverwrite`. All admin commands require `Administrator` permission.

## Admin Commands

### `/setup`

Initialize guild verification configuration and create the verify button embed.

| Option | Type | Required | Description |
|---|---|---|---|
| `domain` | String | Yes | Allowed email domain (e.g. `sspu-opava.cz`) |
| `mode` | String | Yes | `REGEX` or `CSV` |
| `channel` | Channel | Yes | Channel to post the verify button in |
| `subject` | String | No | Email subject (default: "Verification code") |

**Behavior:**
1. Saves GuildConfig (preserves existing `default_role_id`)
2. Posts an embed with a "Verify" button in the specified channel
3. Defaults: code TTL=10min, max attempts=5, rate limit=3/15min

### `/regex`

| Subcommand | Options | Description |
|---|---|---|
| `add` | `pattern` (String, req), `role` (Role, req), `priority` (Integer, opt) | Add regex rule |
| `list` | — | List all rules with ID, pattern, role, priority |
| `remove` | `id` (Integer, req) | Remove rule by ID |

### `/csv`

| Subcommand | Options | Description |
|---|---|---|
| `upload` | `file` (Attachment, req) | Upload CSV (email,class), replaces all existing data |
| `map` | `class` (String, req), `role` (Role, req) | Map class name to Discord role |

### `/ratelimit`

| Option | Type | Required | Description |
|---|---|---|---|
| `count` | Integer | Yes | Max emails (1-3) |
| `window` | Integer | Yes | Window in minutes (15-60) |

### `/verifiedrole`

| Subcommand | Options | Description |
|---|---|---|
| `set` | `role` (Role, req) | Set default role for all verified users |
| `view` | — | Show current default verified role |
| `clear` | — | Remove default verified role |

### `/backup`

| Subcommand | Options | Description |
|---|---|---|
| `create` | `scope` (String, opt: single/multi), `guild-id` (String, opt) | Create manual backup |
| `restore` | `id` (Integer, req), `guild-id` (String, opt) | Restore from backup |
| `list` | `type` (String, opt: all/manual/scheduled) | List backups |
| `schedule` | `frequency` (String, req: 12h/daily/weekly/biweekly/monthly/3months/6months), `time` (String, opt: HH:MM) | Enable scheduled backups |
| `schedule-off` | — | Disable scheduled backups |
| `delete` | `id` (Integer, req) | Delete a backup record |

## User Commands

### `/language`

| Option | Type | Required | Description |
|---|---|---|---|
| `language` | String | Yes | `en` (English) or `cs` (Čeština) |

Stores preference in `user_locales` table per guild.

### `/help`

Shows an embed listing all available commands categorized as Administrator or User.

## Interaction Components

These are not slash commands but UI components triggered during the verification flow:

| Custom ID | Type | Context |
|---|---|---|
| `btn_verify_start` | Button | Opens email input modal |
| `btn_enter_code` | Button | Opens code input modal |
| `modal_email` | Modal | Email submission (text input `input_email`) |
| `modal_code` | Modal | Code entry (text input `input_code`, min 6, max 6 chars) |