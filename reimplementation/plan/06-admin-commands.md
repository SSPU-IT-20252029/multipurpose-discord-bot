# Session 6 — Admin Commands (Part 1) + Verify Entry Point

> **Goal:** the first three admin command groups (`/setup`, `/regex`, `/csv`) plus the verify
> button and email modal. `/ratelimit`, `/verifiedrole`, `/language`, and the code modal are in
> Session 7.
>
> **Parity target:** Go `cmd/bot/main.go` handlers for setup/regex/csv + `handleComponent`
> (`btn_verify_start`) + `handleModal` (`modal_email`).

## Implemented — parity & API notes (verified)

- **poise 0.6 does NOT dispatch components/modals as commands.** Buttons and modal submissions
  are handled via `FrameworkOptions::event_handler` (called for every `FullEvent`), mirroring Go's
  `onInteractionCreate` switch. Custom ids are constants in `src/bot/interactions.rs`.
- **Import namespace:** use `serenity::all` for model/builder types. `poise::serenity_prelude`
  re-exports `serenity::all::*` but from poise's own serenity dependency (fewer features) — the
  direct `serenity` crate is safer for `Context`/`FullEvent`/`CreateModal` etc.
- **Interaction variants** are `Interaction::Component` / `Interaction::Modal` (serenity 0.12
  renamed `MessageComponent`/`ModalSubmit`).
- **Choices:** `/setup mode` uses `#[choices("REGEX", "CSV")] mode: &'static str` — a String option
  whose choice **value equals the label**. Values match Go exactly ("REGEX"/"CSV"); labels differ
  cosmetically ("REGEX" vs Go's "Regex Matching").
- **`respond_ok`/`respond_err`** take `ApplicationContext` (Copy) and use `poise::CreateReply`.
- **Setup embed/button:** `ChannelId::send_message(&http, CreateMessage::new().embed(CreateEmbed).components(Vec<CreateActionRow>))` — serenity 0.12 builder methods take values, not closures.
- **Modal + message responses** built via `CreateModal::new(custom_id, title).components(...)`,
  `CreateInteractionResponse::Modal(...)` / `::Message(CreateInteractionResponseMessage)`, sent
  with `ComponentInteraction::create_response` / `ModalInteraction::create_response`.
- **Attachment download** via `reqwest::get(&file.url)` (Go `http.Get`).
- **Email modal** reads the input with `poise::find_modal_text(&mut data, INPUT_EMAIL)`.
- Admin gating uses `default_member_permissions = "ADMINISTRATOR"` at the command level (Go
  `DefaultMemberPermissions`); no runtime check needed (Discord enforces it).

---

## 6.1 Admin gating (shared)

All admin commands set `default_member_permissions = Administrator` at registration so Discord
hides them from non-admins, matching Go's `DefaultMemberPermissions`.

```rust
#[poise::command(slash_command, default_member_permissions = "ADMINISTRATOR")]
async fn setup(/* … */) {}
```

Runtime check anyway (parity with Go's explicit check in each handler): verify
`ctx.author().permissions(ctx)` contains `ADMINISTRATOR`; reply localized "no permission"
ephemeral otherwise.

## 6.2 `/setup`

```rust
#[poise::command(slash_command, default_member_permissions = "ADMINISTRATOR")]
pub async fn setup(
    ctx: ApplicationContext<'_, Bot, Error>,
    domain: String,
    mode: String,                       // "REGEX" | "CSV"
    channel: serenity::model::channel::Channel,
    #[description = "email subject"] subject: Option<String>,
) -> Result<(), Error> {
    let t = i18n::get(ctx.data().locale(&ctx.into()).await);

    let mode = parse_mode(&mode).map_err(|_| localized_invalid_mode(t))?;
    let guild_id = ctx.interaction.guild_id.context("no guild")?;

    // Preserve default_role_id across re-setup (parity with Go).
    let existing_default = ctx.data().store.get_guild_config(&guild_id.to_string())?
        .map(|c| c.default_role_id).unwrap_or_default();

    let cfg = GuildConfig {
        guild_id: guild_id.to_string(),
        verify_channel_id: channel.id().to_string(),
        domain,
        mode,
        subject: subject.unwrap_or_default(),
        code_ttl_ns: DEFAULT_TTL_NS,             // 10 min in nanoseconds
        max_attempts: 5,
        rate_limit_count: 3,
        rate_limit_window_min: 15,
        default_role_id: existing_default,
    };
    ctx.data().store.save_guild_config(&cfg)?;

    // Post the verify embed + button into the chosen channel (public message).
    channel.id().send_message(ctx.http(), |m| m
        .embed(|e| e.title(t.verify_embed_title).description(t.verify_embed_desc))
        .components(|c| c.create_action_row(|r| r
            .create_button(|b| b.custom_id("btn_verify_start").label(t.verify_btn).style(ButtonStyle::Primary)))))  // style parity: check Go (Primary)
        .await?;

    // Ephemeral confirmation to the admin.
    ephemeral_reply(ctx, format!("{} {}", t.setup_done, channel.name)).await
}
```

**Parity points to verify against Go:**
- Button style used by Go (Primary vs Success) — mirror exactly.
- Embed title/description copy from Go.
- Whether `/setup` also posts into the **interaction channel** an ack vs just the embed channel.

## 6.3 `/regex` group

```rust
#[poise::command(slash_command, subcommands("add", "list", "remove"), default_member_permissions = "ADMINISTRATOR")]
async fn regex(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    // group root: show usage
    Ok(())
}

#[poise::command(slash_command)]
async fn add(
    ctx: ApplicationContext<'_, Bot, Error>,
    pattern: String,
    role: serenity::model::Role,
    priority: Option<i64>,
) -> Result<(), Error> {
    // validate regex compiles (parity: Go may accept any string; if Go compiled it, do same)
    // store: add_regex_rule(guild, pattern, role.id, priority.unwrap_or(0))
    // reply ephemeral success with rule id
}

#[poise::command(slash_command)]
async fn list(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    // rows: `{id}` `{pattern}` → `@role` (prio {priority}); empty state localized
    // ephemeral, one embed; chunk if >25 rules (embed field limit) — parity: check Go truncation
}

#[poise::command(slash_command)]
async fn remove(ctx: ApplicationContext<'_, Bot, Error>, id: i64) -> Result<(), Error> {
    // store.remove_regex_rule(id, guild); reply success/failure (rule not found → localized error)
}
```

## 6.4 `/csv` group

```rust
#[poise::command(slash_command, subcommands("upload", "map"), default_member_permissions = "ADMINISTRATOR")]
async fn csv(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> { Ok(()) }

#[poise::command(slash_command)]
async fn upload(ctx: ApplicationContext<'_, Bot, Error>, file: serenity::model::Attachment) -> Result<(), Error> {
    // 1. Download the attachment bytes via reqwest (parity: Go http.Get CDN URL).
    let bytes = ctx.http().get_attachment(&file.url).await?;

    // 2. Parse CSV: use the `csv` crate; columns 0=email, 1=class.
    //    Parity: Go does NOT skip the header row; empty email/class skipped.
    // 3. Destructive replace: store.clear_csv_emails(guild) then insert rows.
    // 4. Ephemeral reply: rows imported (and if header imported, count parity).
}

#[poise::command(slash_command)]
async fn map(
    ctx: ApplicationContext<'_, Bot, Error>,
    class: String,
    role: serenity::model::Role,
) -> Result<(), Error> {
    // store.upsert_csv_mapping(guild, class, role.id)
    // ephemeral success
}
```

> `serenity`'s `http::Http::get_attachment(&url)` performs an authenticated CDN download —
> cleaner than raw `reqwest` for parity.

## 6.5 Verify entry point (button + email modal)

Custom ids are constants shared by handler and tests:

```rust
pub const BTN_VERIFY_START: &str = "btn_verify_start";
pub const BTN_ENTER_CODE:  &str = "btn_enter_code";
pub const MODAL_EMAIL:     &str = "modal_email";
pub const INPUT_EMAIL:     &str = "input_email";
pub const MODAL_CODE:      &str = "modal_code";
pub const INPUT_CODE:      &str = "input_code";
```

```rust
#[poise::command(context_menu_command = "verify_start")]   // poise component handler
async fn verify_start(ctx: ComponentContext<'_, Bot, Error>) -> Result<(), Error> {
    // Ensure the guild has a config (parity: if missing, localized "not set up").
    // Show the email modal:
    ctx.send(|m| m.modal(|modal| modal
        .custom_id(MODAL_EMAIL)
        .title(t.email_modal_title)
        .text_input(|ti| ti
            .custom_id(INPUT_EMAIL)
            .label(t.email_input_label)
            .placeholder(t.email_placeholder)
            .required(true)
            .text_input_style(TextInputStyle::Short))))
    .await?;
    Ok(())
}
```

`submit_email` (modal handler) calls `verify.start(...)`:

```rust
#[poise::command(context_menu_command = "submit_email")]
async fn submit_email(ctx: ModalContext<'_, Bot, Error>) -> Result<(), Error> {
    let email = parse_input(&ctx.interaction.data.components, INPUT_EMAIL);
    let (guild_id, discord_id) = ids(ctx);
    // Re-fetch locale (parity: user may have changed /language mid-flow).
    let t = i18n::get(locale_for(&store, guild, user));
    match ctx.data().verify.start(guild, user, &email, t).await {
        Ok(outcome) => {
            // Localized "code sent" + Enter-Code button (ephemeral).
            ctx.send(|m| m
                .content(format!("{} ({} {})", t.code_sent, t.expires_in, fmt_ttl(outcome.expires_at)))
                .components(|c| c.create_action_row(|r| r
                    .create_button(|b| b.custom_id(BTN_ENTER_CODE).label(t.enter_code_btn).style(ButtonStyle::Primary))))
                .ephemeral(true)).await?;
        }
        Err(e) => {
            let msg = match e {
                StartError::MissingConfig => t.err_missing_config,
                StartError::InvalidDomain => t.err_invalid_domain,
                StartError::NotActive => t.err_not_active,
                StartError::EmailAlreadyUsed => t.err_email_already_used,
                StartError::RateLimited => t.err_rate_limited,
                StartError::SendFailed => t.err_send_failed,
                _ => t.err_generic,
            };
            ctx.send(|m| m.content(msg).ephemeral(true)).await?;
        }
    }
    Ok(())
}
```

> **Important parity note:** Go re-uses the *interaction* to create the modal (no deferred
> response). With poise, `ctx.send(|m| m.modal(...))` does this natively. Do **not** call
> `defer` before showing a modal.

---

## 6.6 Session completion checklist

- [ ] `/setup` saves config preserving `default_role_id`, posts verify embed + button
- [ ] `/regex add|list|remove` complete with localized messages
- [ ] `/csv upload|map` complete; upload is destructive-replace; header imported (parity)
- [ ] `btn_verify_start` opens `modal_email`
- [ ] `modal_email` → `verify.start` handles all error variants with localized ephemeral replies
- [ ] `--debug` logs interaction details
- [ ] clippy/fmt clean
