# Session 5 — Discord Gateway & Core Dispatch

> **Goal:** wire the bot together: build the `Bot` state, connect to the gateway with the exact
> intents, register all slash commands globally on `on_ready`, and route interactions. Include
> `/help` so there is an end-to-end command to test.
>
> **Parity target:** Go `cmd/bot/main.go` (onReady, onInteractionCreate, help) + intents.

---

## 5.1 Bot state

```rust
pub struct Bot {
    pub store: Store,
    pub mailer: Mailer,
    pub verify: VerifyService,
    pub backup: BackupService,          // Session 8
    pub debug: bool,
}

impl Bot {
    pub fn locale(&self, ctx: &Context<'_>) -> i18n::Locale {
        // guild + author id → stored preference (default En)
        let (guild_id, user_id) = match ctx {
            Context::Application(ctx) => (ctx.interaction.guild_id.clone(), ctx.interaction.user.id.to_string()),
            Context::Component(ctx)   => (ctx.interaction.guild_id.clone(), ctx.interaction.user.id.to_string()),
            Context::Modal(ctx)       => (ctx.interaction.guild_id.clone(), ctx.interaction.user.id.to_string()),
            _ => (None, String::new()),
        };
        match (guild_id, user_id.is_empty()) {
            (Some(g), false) => i18n::locale_for(&self.store, &g, &user_id),
            _ => i18n::Locale::En,
        }
    }
}
```

## 5.2 Gateway intents (exact parity)

| Go intent | Serenity equivalent | Reason |
|---|---|---|
| `Guilds` | `GatewayIntents::GUILDS` | slash commands, guild structs |
| `GuildMembers` | `GatewayIntents::GUILD_MEMBERS` | member info for verification |
| `GuildModeration` | `GatewayIntents::GUILD_MODERATION` | bans for backup capture |
| `GuildEmojis` | `GatewayIntents::GUILD_EMOJIS_AND_STICKERS` | emoji backup |
| `GuildBans` | `GatewayIntents::GUILD_BANS` | ban list for backup |

```rust
fn intents() -> GatewayIntents {
    GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MODERATION
        | GatewayIntents::GUILD_EMOJIS_AND_STICKERS
        | GatewayIntents::GUILD_BANS
}
```

> **Deployment note (parity):** Server Members Intent must be enabled in the Developer Portal,
> same as the Go bot. Document this in the Docker/CI session and README.

## 5.3 Framework / global command registration

With `poise`, every handler `#[command]` is auto-registered in the command list. On `on_ready`
we do a **global bulk overwrite** to guarantee the exact command surface (parity with
`ApplicationCommandBulkOverwrite`):

```rust
let commands = vec![
    help(), setup(), regex(), csv(), ratelimit(), verifiedrole(),
    language(), backup(),   // add as each session lands
];

let framework = poise::Framework::builder()
    .options(poise::FrameworkOptions {
        commands,
        on_error: |err| Box::pin(on_error(err)),
        ..Default::default()
    })
    .token(cfg.discord.token.clone())
    .intents(intents())
    .setup(|ctx, ready, framework| Box::pin(async move {
        // Global registration: fetch each command's create_data and bulk-overwrite.
        // poise exposes `framework.commands`; construct Vec<CreateApplicationCommand>.
        let global_commands: Vec<_> = framework.commands.iter()
            .map(|c| c.create_command_as_global(&ctx.serenity_context))
            .collect();
        ctx.http.create_global_application_commands(global_commands).await?;

        let bot = Arc::new(Bot::new(cfg.clone()));
        tracing::info!("logged in as {}", ready.user.name);
        Ok(bot)
    }))
    .build();
```

> **Why bulk overwrite (parity):** the Go bot *replaces* all global commands on startup so stale
> command definitions never linger. `poise` normally registers lazily; an explicit bulk
> overwrite preserves the exact Go behavior. Guard against "command already registered" races by
> bulk-overwriting rather than per-command.

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
async fn on_error(error: poise::FrameworkError<'_, Bot, error::Error>) {
    match error {
        FrameworkError::Command { error, ctx, .. } => {
            // Log server error; try to DM or ephemeral-reply a generic "something broke"
            // using the user's locale if the ctx still permits a reply.
        }
        _ => tracing::error!("unhandled framework error: {error:?}"),
    }
}
```

Verification errors (`VerifyError`) are *not* surfaced through `on_error` — handlers catch them
and reply with the localized message (see Sessions 6–7).

## 5.6 Ephemeral reply helper

```rust
/// Reply ephemeral using the user's locale for the message.
pub async fn ephemeral_reply<T, M>(ctx: poise::ApplicationContext<'_, Bot, error::Error>, content: M) -> Result<(), error::Error>
where T: Into<serenity::model::channel::Message>, M: Into<String>,
{
    ctx.send(|b| b.content(content).ephemeral(true)).await?;
    Ok(())
}
```

## 5.7 `/help` command

Parity: ephemeral embed listing all commands, split into **Administrator** (setup, regex, csv,
ratelimit, verifiedrole, backup) and **User** (language, help), with localized descriptions.

```rust
#[poise::command(slash_command, category = "User")]
pub async fn help(ctx: poise::ApplicationContext<'_, Bot, error::Error>) -> Result<(), error::Error> {
    let t = i18n::get(ctx.data().locale(&ctx.into()).await);
    // build embed:
    //   title: t.help_title, color: brand, fields per group with t.*desc
    ctx.send(|b| b.embed(|e| {
        e.title(t.help_title)
         .field(t.help_admin_title, admin_lines(t), false)
         .field(t.help_user_title, user_lines(t), false)
    }).ephemeral(true)).await?;
    Ok(())
}
```

## 5.8 `main.rs` final wiring (this session)

```rust
#[tokio::main]
async fn main() -> Result<(), error::Error> {
    let cli = Cli::parse();
    init_logging(cli.debug);

    let cfg = config::Config::load(&cli.config)?;
    let store = store::Store::open(&cfg.storage.dsn)?;
    let mailer = mailer::Mailer::new(cfg.email.api_key.clone(), cfg.email.from.clone());
    let verify = verify::VerifyService::new(store.clone(), mailer.clone());

    // Session 8 adds: backup service + scheduler task via tokio::spawn.

    framework.run(shard_manager).await?;
    Ok(())
}
```

---

## 5.9 Manual verification checklist (requires Discord)

- [ ] Bot starts, logs in, global commands appear in a test server **and** globally
- [ ] `Server Members Intent` note confirmed in Developer Portal
- [ ] `/help` replies ephemeral, localized
- [ ] Commands registered via bulk overwrite replace stale definitions
- [ ] `--debug` flag enables gateway debug logs
- [ ] Unknown interaction types are logged, not crashed
