# Internationalization (i18n) System

## Overview

The bot has built-in bilingual support for English and Czech. All user-facing strings — command descriptions, error messages, email templates — are stored in translation structs. Users can switch language per-guild via `/language`.

## Implementation

The system uses a **static translation map** — no runtime loading, no external files.

### Locale Type

```go
type Locale string
const LocaleEN Locale = "en"
const LocaleCS Locale = "cs"
```

### Parser

`ParseLocale(s string)` handles these inputs for Czech:
- `cs`, `cz`, `cs-cz`, `cs_CZ`, `czech`
- Everything else defaults to English

### Translations Struct

A single flat struct with ~140 string fields covering:
- Command names and descriptions (e.g. `SetupDesc`, `RegexDesc`)
- Option labels (e.g. `SetupDomain`, `RegexPattern`)
- Button labels and modal titles (e.g. `VerifyBtn`, `CodeModalTitle`)
- Success/error messages (e.g. `VerifySuccess`, `ErrWrongCodeFmt`)
- Email template parts (e.g. `EmailHello`, `EmailCodeFor`)
- Backup-related strings

### Selection Logic

```go
func Get(locale Locale) Translations {
    if t, ok := translations[locale]; ok {
        return t
    }
    return en  // fallback
}
```

### User Preference Storage

- Stored in `user_locales` table: `(guild_id, user_id, locale)`
- Set via `/language <en|cs>` command
- Default: English (when no preference found)
- Preference is per-guild: a user can have different languages on different servers

## Usage in Code

1. Determine locale at interaction time:
   ```go
   func (b *Bot) getLocale(i *discordgo.InteractionCreate) i18n.Locale {
       locale, _, err := b.store.GetUserLocale(ctx, i.GuildID, i.Member.User.ID)
       if err != nil || locale == "" { return i18n.LocaleEN }
       return i18n.ParseLocale(locale)
   }
   ```
2. Get translations: `t := i18n.Get(locale)`
3. Use format strings: `fmt.Sprintf(t.ErrWrongCodeFmt, wce.Remaining)`
4. For the email verification flow, the user's stored locale is fetched again inside the modal handler to handle cases where the user changed language between starting verification and entering the code

## Current Scope

- **English**: complete, authoritative
- **Czech**: complete translation of all strings
- No other languages are planned