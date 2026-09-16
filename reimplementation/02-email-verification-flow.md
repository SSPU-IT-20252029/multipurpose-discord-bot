# Email Verification Flow

## Overview

Users verify their school email through a two-modal interaction. The bot sends a 6-digit code via Resend, then the user enters it back to prove ownership.

## Step-by-Step

### Phase 1: Setup (Admin)

1. Admin runs `/setup domain:<domain> mode:<REGEX|CSV> channel:<#channel> [subject:<subject>]`
2. Bot saves `GuildConfig` to database
3. Bot posts an embed with a "Verify" button (`btn_verify_start`) in the specified channel

### Phase 2: Email Submission (User)

1. User clicks the "Verify" button
2. Discord shows a modal (`modal_email`) with a single text input for email
3. User submits their email address
4. Bot calls `verify.Service.Start()`

#### `Start()` validation chain:

1. Load `GuildConfig` — fail with `ErrMissingConfig` if missing
2. Extract domain from email (`parts[1]`) — fail with `ErrInvalidDomain` if it doesn't match the configured domain
3. Call `resolveRole()` to check if the email would match any rule — fail with `ErrNotActive` if no rule matches (catches invalid emails early, before sending a code)
4. Check if email is already bound to a different Discord user — fail with `ErrEmailAlreadyUsed`
5. Check rate limit: count sends in the last `RateLimitWindow` — fail with `ErrRateLimited` if at or over `RateLimitCount`
6. Generate random 6-digit code via `crypto/rand`
7. Store pending code (SHA256 hashed) in `pending_codes` table
8. Send email via Resend with code, TTL, and locale-appropriate templates
9. Log the send in `send_log` for rate limit tracking
10. Respond ephemerally with "Code sent" message and an "Enter Code" button (`btn_enter_code`)

### Phase 3: Code Entry (User)

1. User clicks "Enter Code" button
2. Discord shows a modal (`modal_code`) with a text input (min 6, max 6 chars)
3. User submits the 6-digit code
4. Bot calls `verify.Service.Confirm()`

#### `Confirm()` validation chain:

1. Load pending record from `pending_codes` — fail with `ErrNoPending` if none
2. Load `GuildConfig` — fail with `ErrMissingConfig`
3. Check expiry (`now.After(pending.ExpiresAt)`) — delete pending, fail with `ErrExpired`
4. Constant-time compare SHA256 of user input against stored hash
   - **Mismatch**: increment attempts, if `attempts >= MaxAttempts` delete pending and fail `ErrTooManyAttempts`, else fail `WrongCodeError{Remaining}`
   - **Match**: proceed
5. Call `resolveRole()` again to determine which role(s) to assign
6. Save verified user to `verified_users` table
7. Delete pending code
8. Assign roles via `GuildMemberRoleAdd`:
   - The role resolved from email/class matching
   - Plus `DefaultRoleID` if configured and different from the resolved role
9. Respond ephemerally with success message

## Code Generation

```go
func generateCode() (string, error) {
    n, err := rand.Int(rand.Reader, big.NewInt(1_000_000))
    // returns zero-padded 6-digit string, e.g. "004213"
}
```

Codes are SHA256-hashed before storage. The input is normalized by stripping spaces.

## Rate Limiting

- Configurable per guild: `RateLimitCount` (1-3) and `RateLimitWindow` (15-60 minutes)
- Tracked via `send_log` table: INSERT on send, DELETE old entries (>2h ago) on each send
- `CountSendsSince(guildID, discordID, now - window)` returns the count
- Default: 3 sends per 15 minutes
- Admin can override via `/ratelimit count:<1-3> window:<15-60>`

## Error Reference

| Error | Condition |
|---|---|
| `ErrMissingConfig` | Guild not set up yet |
| `ErrInvalidDomain` | Email domain != configured domain |
| `ErrNotActive` | No regex/csv rule matches the email |
| `ErrEmailAlreadyUsed` | Email exists in `verified_users` for a different Discord user |
| `ErrRateLimited` | Too many sends in the time window |
| `ErrNoPending` | No pending code for this user |
| `ErrExpired` | Code TTL elapsed |
| `ErrTooManyAttempts` | Wrong code entered too many times |
| `ErrSendFailed` | Resend API returned an error |
| `WrongCodeError` | Code mismatch (includes remaining attempts count) |