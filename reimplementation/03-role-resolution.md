# Role Resolution

## Overview

After email ownership is confirmed, the bot determines which Discord role(s) to assign. Two mutually exclusive modes exist per guild: **Regex** or **CSV**. The mode is set during `/setup` and stored in `guilds.mode`.

Both modes return a single "primary" role ID. Additionally, if `guilds.default_role_id` is set and is different from the primary role, it is appended to the assignment list.

---

## Regex Mode

### How It Works

1. Load all `regex_rules` for the guild, ordered by `priority DESC`
2. Iterate through rules in order
3. For each rule, test `regexp.MatchString(pattern, email)`
4. Return the `role_id` of the **first matching rule**
5. If no rule matches, return `ErrNotActive`

### Rule Management

| Command | Effect |
|---|---|
| `/regex add pattern:<regex> role:<@role> [priority:<int>]` | Insert new rule |
| `/regex list` | Show all rules (ID, pattern, role, priority) |
| `/regex remove id:<int>` | Delete rule by ID |

### Characteristics

- Patterns are standard Go regex (RE2 syntax)
- Priority is user-defined; higher values are evaluated first
- Matching is done against the **full email string** (e.g. `student@sspu-opava.cz`)
- Rules are guild-scoped, isolated between servers

---

## CSV Mode

### How It Works

1. Two data sources are involved:
   - **csv_emails**: uploaded CSV data mapping individual emails to class names
   - **csv_mappings**: maps class names to Discord role IDs
2. When resolving:
   ```sql
   SELECT m.role_id
   FROM csv_emails e
   JOIN csv_mappings m
     ON e.guild_id = m.guild_id
    AND e.class_name = m.class_name
   WHERE e.guild_id = ? AND e.email = ?
   ```
3. If no row returned, return `ErrNotActive`

### CSV Upload Format

```
email,class
student1@domain.com,3A
student2@domain.com,3B
```

### Management Commands

| Command | Effect |
|---|---|
| `/csv upload file:<attachment>` | Replace all CSV data for the guild (clears existing first) |
| `/csv map class:<name> role:<@role>` | Map a class name to a role (upsert) |

### Characteristics

- CSV upload is destructive: `DELETE FROM csv_emails WHERE guild_id = ?` runs before inserting new rows
- The bot downloads the file from Discord's CDN and parses it as CSV
- Only columns 0 (email) and 1 (class) are used; header row is imported as data (there is no skip-header logic)
- Empty emails or class names are skipped during import