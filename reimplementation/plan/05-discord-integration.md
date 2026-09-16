# Session 5 — Discord Gateway & Core Dispatch

> **Goal:** wire the bot together: build the `Bot` state, connect to the gateway with the exact
> intents, register all slash commands globally on `on_ready`, and route interactions. Include
> `/help` so there is an end-to-end command to test.
>
> **Parity target:** Go `cmd/bot/main.go` (onReady, onInteractionCreate, help) + intents.

---

## 5.1 Bot state (implemented)

```rust
pub struct Bot {
    pub store: Store,
    pub mailer: Mailer,
    pub verify: Arc<VerifyService<Mailer>>,   // generic service behind Arc
    pub debug: bool,
    // Session 8 adds `backup`.
}
```

Locale resolution avoids consuming poise `Context` — the handler extracts ids and calls:

```rust
// Bot::locale(&self, guild_id: Option<u64>, user_id: u64) -> i18n::Locale
//   → store.get_user_locale(g.to_string(), user_id.to_string()) or En
```

Handlers pass `ctx.data().locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get())`.

## 5.2 Gateway intents (implemented)

| Go intent | Serenity equivalent | Reason |
|---|---|---|
| `Guilds` | `GatewayIntents::GUILDS` | slash commands, guild structs |
| `GuildMembers` | `GatewayIntents::GUILD_MEMBERS` | member info for verification |
| `GuildModeration` | `GatewayIntents::GUILD_MODERATION` | bans + moderation (Go lists `GuildBans` separately — **merged into `GUILD_MODERATION` in serenity**) |
| `GuildEmojis` | `GatewayIntents::GUILD_EMOJIS_AND_STICKERS` | emoji backup |

```rust
pub fn intents() -> GatewayIntents {
    GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MODERATION
        | GatewayIntents::GUILD_EMOJIS_AND_STICKERS
}
```

> **Deployment note (parity):** Server Members Intent must be enabled in the Developer Portal,
> same as the Go bot. Document this in the Docker/CI session and README.

## 5.3 Framework / global command registration (implemented)

With `poise`, every handler `#[command]` is auto-registered in the command list. On startup
(poise `setup`, run on `Ready`) we do a **global bulk overwrite** to guarantee the exact command
surface (parity with `ApplicationCommandBulkOverwrite`):

```rust
let framework = poise::Framework::builder()
    .options(poise::FrameworkOptions {
        commands: vec![commands::help()],   // grows each session
        on_error,
        ..Default::default()
    })
    .setup(move |ctx, ready, framework| Box::pin(async move {
        // Bulk overwrite: PUT /applications/{id}/commands replaces all global commands.
        poise::builtins::register_globally(&ctx.http, &framework.options().commands).await?;
        tracing::info!(user = %ready.user.name, "logged in");
        Ok(Bot { store, mailer, verify, debug })
    }))
    .build();
```

`main` then creates the serenity client — **poise 0.6 moved token + intents to the serenity
`Client::builder`** (not the poise builder):

```rust
let mut client = serenity::Client::builder(token, bot::intents())
    .framework(framework)
    .await?;
// ctrl-c → client.shard_manager.shutdown_all() (Go signal.Notify parity)
client.start_autosharded().await?;
```

> **Why bulk overwrite (parity):** `register_globally` → `set_global_commands` uses `PUT`, which
> replaces all global commands so stale definitions never linger — exactly Go's behavior.

## 5.4 Interaction dispatch

`poise` routes slash commands, components, and modals to annotated handlers automatically.
The dispatch map (parity with Go `onInteractionCreate` → switch):

| Interaction | Custom id / command | Handler |
|---|---|---|
| slash command `setup` | — | `commands::setup` |
| slash command `regex` | — | `commands::regex` (subcommand group) |
| slash command `csv` | — | `commands::csv` (subcommand group) |
| slash command `ratelimit` | — | `commands::ratelimit` |
| slash command `verifiedrole` | — | `commands::verifiedrole` |
| slash command `backup` | — | `commands::backup` (subcommand group) |
| slash command `language` | — | `commands::language` |
| slash command `help` | — | `commands::help` |
| button `btn_verify_start` | — | `components::verify_start` |
| button `btn_enter_code` | — | `components::enter_code` |
| modal `modal_email` | — | `components::submit_email` |
| modal `modal_code` | — | `components::submit_code` |

Any unknown/unhandled interaction → log + (if deferred) edit, else ignore (parity).

## 5.5 Error handling (on_error)

```rust
fn on_error(error: poise::FrameworkError<'_, Bot, error::Error>) -> BoxFuture<'_, ()> {
    Box::pin(async move {
        match error {
            FrameworkError::Command { error, ctx, .. } => {
                tracing::error!(%error, "command failed");
                // best-effort ephemeral "An error occurred."
                let _ = ctx.send(CreateReply::default().content("An error occurred.").ephemeral(true)).await;
            }
            other => tracing::error!("unhandled framework error: {other}"),
        }
    })
}
```

> `FrameworkError` implements `Display` when `E: Display`, so `{other}` avoids requiring
> `Bot: Debug`.

Verification errors (`VerifyError`) are *not* surfaced through `on_error` — handlers catch them
and reply with the localized message (see Sessions 6–7).

## 5.6 Ephemeral reply helpers (implemented)

```rust
// poise::CreateReply (not a closure); ctx.send consumes a copy of the ctx.
pub async fn respond_ok(ctx: ApplicationContext<'_, Bot, Error>, msg: String) -> Result<(), Error> {
    ctx.send(CreateReply::default().content(format!("✅ {msg}")).ephemeral(true)).await?;
    Ok(())
}
pub async fn respond_err(ctx: ApplicationContext<'_, Bot, Error>, msg: String) -> Result<(), Error> {
    ctx.send(CreateReply::default().content(format!("❌ {msg}")).ephemeral(true)).await?;
    Ok(())
}
```

Parity: Go `respondOK` / `respondErr` — `✅ ` / `❌ ` prefixes, ephemeral.

## 5.7 `/help` command (implemented)

Parity: Go `cmdHelp` — ephemeral embed, title `HelpText`, color `0x3b82f6`, description is the
**exact** Go layout: `HelpClickHint + "\n\n" + "**Administrator**\n"` + `/setup /regex /csv
/ratelimit /verifiedrole` lines + `"\n\n**User**\n"` + `/language /help`. **`/backup` is NOT
listed** (Go parity). Uses `CreateEmbed::default().title(...).description(...).color(0x3b82f6)`
(no field-based layout — Go used one description string).

```rust
#[poise::command(slash_command)]
pub async fn help(ctx: ApplicationContext<'_, Bot, Error>) -> Result<(), Error> {
    let t = i18n::get(ctx.data().locale(ctx.guild_id().map(|g| g.get()), ctx.author().id.get()));
    let description = format!(
        "{hint}\n\n**{admin}**\n`/setup` - {setup}\n`/regex` - {regex}\n`/csv` - {csv}\n\
         `/ratelimit` - {ratelimit}\n`/verifiedrole` - {verifiedrole}\n\n**{user}**\n\
         `/language` - {language}\n`/help` - {help}",
        hint = t.help_click_hint, admin = t.help_admin_title, /* ... */
    );
    ctx.send(CreateReply::default()
        .embed(CreateEmbed::default().title(t.help_text).description(description).color(0x3b82f6))
        .ephemeral(true)).await?;
    Ok(())
}
```

## 5.8 `main.rs` final wiring (implemented)

```rust
#[tokio::main]
async fn main() -> Result<(), error::Error> {
    let cli = Cli::parse();
    init_logging(cli.debug);

    let cfg = config::Config::load(&cli.config)?;
    let store = store::Store::open(&cfg.storage.dsn)?;
    let mailer = mailer::Mailer::new(cfg.email.api_key.clone(), cfg.email.from.clone());
    let verify = verify::VerifyService::new(store.clone(), mailer.clone());

    let token = cfg.discord.token.clone();
    let framework = bot::build(store, mailer, verify, cli.debug);

    let mut client = serenity::Client::builder(token, bot::intents()).framework(framework).await?;
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;       // Go signal.Notify(SIGINT/SIGTERM)
        shard_manager.shutdown_all().await;
    });
    client.start_autosharded().await?;
    Ok(())
}
```

> Session 8 adds: backup service + scheduler task via `tokio::spawn`.

---

## 5.9 Manual verification checklist (requires Discord)

- [ ] Bot starts, logs in, global commands appear in a test server **and** globally
- [ ] `Server Members Intent` note confirmed in Developer Portal
- [ ] `/help` replies ephemeral, localized
- [ ] Commands registered via bulk overwrite replace stale definitions
- [ ] `--debug` flag enables gateway debug logs
- [ ] Unknown interaction types are logged, not crashed
