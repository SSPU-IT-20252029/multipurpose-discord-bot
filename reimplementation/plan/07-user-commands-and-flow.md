# Session 7 — Remaining Admin Commands + User Flow

> **Goal:** `/ratelimit`, `/verifiedrole`, `/language`, the **Enter-Code button + code modal**,
> and full end-to-end verification. This completes every interaction handler.
>
> **Parity target:** Go `cmd/bot/main.go` ratelimit/verifiedrole/language handlers, `handleModal`
> (`modal_code` → `verify.Confirm()`), and the role-assignment step
> (`GuildMemberRoleAdd`).

---

## 7.1 `/ratelimit`

```rust
#[poise::command(slash_command, default_member_permissions = "ADMINISTRATOR")]
pub async fn ratelimit(
    ctx: ApplicationContext<'_, Bot, Error>,
    count: i64,                 // 1..=3
    window: i64,                // 15..=60 minutes
) -> Result<(), Error> {
    // clamp/validate (parity: Go validated 1-3 and 15-60; confirm behavior on invalid input)
    // load cfg (must exist → localized "not set up"), update rate_limit_count/window, save.
    // ephemeral success: "Rate limit: N / M min"
}
```

## 7.2 `/verifiedrole` group

```rust
#[poise::command(slash_command, subcommands("set", "view", "clear"), default_member_permissions = "ADMINISTRATOR")]
async fn verifiedrole(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> { Ok(()) }

async fn set(ctx, role: Role)  { /* update cfg.default_role_id = role.id; ephemeral success */ }
async fn view(ctx)             { /* show current role or "none" localized */ }
async fn clear(ctx)            { /* cfg.default_role_id = ""; ephemeral success */ }
```

## 7.3 `/language`

User-facing command — **no admin gate**.

```rust
#[poise::command(slash_command)]
pub async fn language(
    ctx: ApplicationContext<'_, Bot, Error>,
    language: i18n::Locale,     // poise enum choice: en / cs
) -> Result<(), Error> {
    let (guild_id, user_id) = ids(ctx);
    ctx.data().store.set_user_locale(&guild_id, &user_id, language.code())?;
    // Ephemeral success in the *newly selected* language.
}
```

**Poise enum-choice trick:** implement a `ChoiceParameter` for `Locale` so the option renders a
`en`/`cs` choice list, matching the Go string choices `en`/`cs` and keeping the DB string
identical (`"en"`/`"cs"`).

## 7.4 Enter-Code button + code modal

```rust
#[poise::command(context_menu_command = "enter_code")]
async fn enter_code(ctx: ComponentContext<'_, Bot, Error>) -> Result<(), Error> {
    // Show code modal:
    ctx.send(|m| m.modal(|modal| modal
        .custom_id(MODAL_CODE)
        .title(t.code_modal_title)
        .text_input(|ti| ti
            .custom_id(INPUT_CODE)
            .label(t.code_input_label)
            .min_length(6).max_length(6)
            .required(true)
            .text_input_style(TextInputStyle::Short))))
    .await?;
    Ok(())
}
```

## 7.5 Code submission → `verify.confirm` → role assignment

```rust
#[poise::command(context_menu_command = "submit_code")]
async fn submit_code(ctx: ModalContext<'_, Bot, Error>) -> Result<(), Error> {
    let code = parse_input(&ctx.interaction.data.components, INPUT_CODE);
    let (guild_id, user_id) = ids(ctx);
    let t = i18n::get(locale_for(&store, guild, user));   // re-fetch (parity)

    match ctx.data().verify.confirm(&guild_id, &user_id, &code, t).await {
        Ok(outcome) => {
            // Assign roles: primary + optional default.
            let mut to_assign = vec![outcome.role.primary.clone()];
            if let Some(def) = &outcome.role.default { to_assign.push(def.clone()); }

            for role_id in to_assign {
                ctx.serenity_context.http.add_member_role(
                    &guild_id, &user_id, &role_id,
                    Some("verified via email"),
                ).await?;
            }

            // Ephemeral success + welcome message (localized, includes email).
            ctx.send(|m| m
                .content(format!("{} {}", t.verify_success, outcome.email))
                .ephemeral(true)).await?;
        }
        Err(e) => {
            let msg = match e {
                ConfirmError::NoPending         => t.err_no_pending,
                ConfirmError::Expired           => t.err_expired,
                ConfirmError::TooManyAttempts   => t.err_too_many_attempts,
                ConfirmError::WrongCode { remaining } => format!(t.err_wrong_code_fmt, remaining),
                ConfirmError::MissingConfig     => t.err_missing_config,
                ConfirmError::NotActive         => t.err_not_active,
                _ => t.err_generic,
            };
            ctx.send(|m| m.content(msg).ephemeral(true)).await?;
        }
    }
    Ok(())
}
```

**Parity points:**
- Role assignment order: primary first, then default (Go calls `GuildMemberRoleAdd` per role).
- On role-assignment HTTP error, the user is **already verified** in DB (Go saved first).
  Decide: reply success with a warning, or surface the error — mirror Go.
- `add_member_role` reason is cosmetic; Go used none or a fixed string — match it.
- **Already-verified edge case:** if `confirm` returns success but roles were already applied
  (duplicate verification), Discord ignores the duplicate add; response remains success.

---

## 7.6 End-to-end flow summary (manual test script)

1. Admin `/setup domain:sspu-opava.cz mode:REGEX channel:#verify subject:...`
   → verify embed + button posted in `#verify`.
2. Admin `/regex add pattern:.*@sspu-opava.cz role:@student`.
3. User clicks **Verify** → email modal → submits email.
4. Bot sends code via Resend → ephemeral "Code sent" + **Enter Code** button.
5. User clicks **Enter Code** → 6-digit modal.
6. Correct code → roles assigned, verified user saved.
7. Wrong code × 5 → "too many attempts" and pending deleted.
8. Re-use email on another account → "email already used".
9. `/language cs` → all subsequent messages Czech; `/language en` back.

---

## 7.7 Session completion checklist

- [ ] `/ratelimit` updates config with validation
- [ ] `/verifiedrole set|view|clear` complete
- [ ] `/language` writes `user_locales`; poise choice enum preserves `en`/`cs` strings
- [ ] `btn_enter_code` → `modal_code` (6-char input) complete
- [ ] `modal_code` → `confirm` → role assignment (primary + default) complete
- [ ] All error variants reply localized & ephemeral
- [ ] End-to-end flow tested on a real server (manual checklist §7.6)
- [ ] clippy/fmt clean
