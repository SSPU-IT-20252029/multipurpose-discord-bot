//! Discord gateway integration: bot state, command registration (global bulk
//! overwrite), interaction dispatch glue, and reply helpers.
//!
//! Parity target: Go `cmd/bot/main.go` (`Bot`, `onReady`, `onInteractionCreate`,
//! `respondOK`/`respondErr`).

pub mod commands;
pub mod interactions;

use crate::error::Error;
use crate::i18n;
use crate::mailer::Mailer;
use crate::store::Store;
use crate::verify::VerifyService;
use poise::{BoxFuture, CreateReply, FrameworkError, FrameworkOptions};
use serenity::all::{self as serenity, GatewayIntents};
use std::sync::Arc;

/// Shared application state, injected into every command via `ctx.data()`.
pub struct Bot {
    pub store: Store,
    pub mailer: Mailer,
    pub verify: Arc<VerifyService<Mailer>>,
    pub debug: bool,
}

impl Bot {
    /// Resolve a user's per-guild locale preference, defaulting to English.
    /// Parity: Go `Bot.getLocale`.
    pub fn locale(&self, guild_id: Option<u64>, user_id: u64) -> i18n::Locale {
        let stored = guild_id.and_then(|g| {
            self.store
                .get_user_locale(&g.to_string(), &user_id.to_string())
                .ok()
                .flatten()
        });
        stored
            .map(|l| i18n::Locale::parse(&l))
            .unwrap_or(i18n::Locale::En)
    }
}

/// Parity: Go `dg.Identify.Intents`. `GUILD_BANS` is folded into
/// `GUILD_MODERATION` in serenity (Go lists both intents explicitly).
pub fn intents() -> GatewayIntents {
    GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MEMBERS
        | GatewayIntents::GUILD_MODERATION
        | GatewayIntents::GUILD_EMOJIS_AND_STICKERS
}

/// Build the poise framework. Global commands are bulk-overwritten on startup
/// (Go `ApplicationCommandBulkOverwrite`), replacing any stale definitions.
/// The token and intents are passed to `serenity::Client::builder` in `main`
/// (poise 0.6 delegates them there).
pub fn build(
    store: Store,
    mailer: Mailer,
    verify: VerifyService<Mailer>,
    debug: bool,
) -> poise::Framework<Bot, Error> {
    let verify = Arc::new(verify);
    poise::Framework::builder()
        .options(FrameworkOptions {
            commands: vec![
                commands::help(),
                commands::setup(),
                commands::regex(),
                commands::csv(),
            ],
            on_error,
            event_handler,
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                // Global bulk overwrite (parity with Go onReady).
                poise::builtins::register_globally(&ctx.http, &framework.options().commands)
                    .await?;
                tracing::info!(
                    user = %ready.user.name,
                    "logged in, {} global commands registered",
                    framework.options().commands.len()
                );
                Ok(Bot {
                    store,
                    mailer,
                    verify,
                    debug,
                })
            })
        })
        .build()
}

/// Global error handler (parity: Go logs interaction errors; here we log and
/// best-effort reply ephemeral so the user sees something).
fn on_error(error: FrameworkError<'_, Bot, Error>) -> BoxFuture<'_, ()> {
    Box::pin(async move {
        match error {
            FrameworkError::Command { error, ctx, .. } => {
                tracing::error!(%error, "command failed");
                if let Err(e) = ctx
                    .send(
                        CreateReply::default()
                            .content("An error occurred.")
                            .ephemeral(true),
                    )
                    .await
                {
                    tracing::warn!(%e, "failed to send error reply");
                }
            }
            other => tracing::error!("unhandled framework error: {other}"),
        }
    })
}

/// Ephemeral success reply. Parity: Go `respondOK` (`"✅ "` prefix).
pub async fn respond_ok(
    ctx: poise::ApplicationContext<'_, Bot, Error>,
    msg: String,
) -> Result<(), Error> {
    ctx.send(
        CreateReply::default()
            .content(format!("✅ {msg}"))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Ephemeral error reply. Parity: Go `respondErr` (`"❌ "` prefix).
pub async fn respond_err(
    ctx: poise::ApplicationContext<'_, Bot, Error>,
    msg: String,
) -> Result<(), Error> {
    ctx.send(
        CreateReply::default()
            .content(format!("❌ {msg}"))
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Bridge for poise: forwards non-slash interactions (buttons + modals) to
/// [`interactions::handle_interaction`]. Slash commands are dispatched by poise
/// itself before this runs (parity: Go `onInteractionCreate`).
fn event_handler<'a>(
    ctx: &'a serenity::Context,
    event: &'a serenity::FullEvent,
    _framework: poise::FrameworkContext<'a, Bot, Error>,
    data: &'a Bot,
) -> BoxFuture<'a, Result<(), Error>> {
    Box::pin(async move {
        if let serenity::FullEvent::InteractionCreate { interaction } = event {
            interactions::handle_interaction(ctx, interaction, data).await?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use rusqlite::Connection;

    fn test_store() -> Store {
        let conn = Connection::open_in_memory().expect("in-memory db");
        let store = Store::from_connection(conn);
        store.migrate().expect("migrate");
        store
    }

    fn test_bot(store: Store) -> Bot {
        let mailer = Mailer::new("re_key".into(), "bot@example.com".into());
        let verify = VerifyService::new(store.clone(), mailer.clone());
        Bot {
            store,
            mailer,
            verify: Arc::new(verify),
            debug: false,
        }
    }

    #[test]
    fn intents_cover_verification_and_backup() {
        let i = intents();
        assert!(i.contains(GatewayIntents::GUILDS));
        assert!(i.contains(GatewayIntents::GUILD_MEMBERS));
        assert!(i.contains(GatewayIntents::GUILD_MODERATION));
        assert!(i.contains(GatewayIntents::GUILD_EMOJIS_AND_STICKERS));
    }

    #[test]
    fn locale_defaults_to_english() {
        let bot = test_bot(test_store());
        assert_eq!(bot.locale(None, 123), i18n::Locale::En);
        assert_eq!(bot.locale(Some(1), 123), i18n::Locale::En);
    }

    #[test]
    fn locale_uses_stored_preference() {
        let store = test_store();
        store.set_user_locale("1", "123", "cs").unwrap();
        let bot = test_bot(store);
        assert_eq!(bot.locale(Some(1), 123), i18n::Locale::Cs);
        assert_eq!(bot.locale(Some(2), 123), i18n::Locale::En);
        assert_eq!(bot.locale(None, 123), i18n::Locale::En);
    }
}
