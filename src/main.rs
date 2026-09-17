//! Binary entry point.
//!
//! Secrets (`DISCORD_TOKEN`, `RESEND_API_KEY`, `EMAIL_FROM`) are loaded from
//! `.env` via `dotenvy` at startup.

use clap::Parser;
use multipurpose_discord_bot::{bot, config, error::Error, mailer, store, verify};
use poise::serenity_prelude::Client;

/// Command-line interface.
///
/// PARITY: the Go bot used single-dash flags (`-config`, `-debug`). clap follows
/// POSIX conventions, so the canonical forms here are `--config` / `--debug`.
#[derive(Parser, Debug)]
#[command(name = "multipurpose-discord-bot", about = "Multipurpose Discord bot")]
pub struct Cli {
    /// Path to the YAML configuration file.
    #[arg(long, default_value = "config.yml")]
    config: String,

    /// Enable verbose debug logging.
    #[arg(long, default_value_t = false)]
    debug: bool,
}

fn init_logging(debug: bool) {
    let filter = if debug { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    init_logging(cli.debug);

    // Load secrets from .env before anything else.
    dotenvy::dotenv().ok();
    let discord_token = std::env::var("DISCORD_TOKEN")
        .map_err(|_| Error::Message("DISCORD_TOKEN not set".into()))?;
    let resend_api_key = std::env::var("RESEND_API_KEY")
        .or_else(|_| std::env::var("RESEND_API_TOKEN"))
        .map_err(|_| Error::Message("RESEND_API_KEY or RESEND_API_TOKEN not set in .env".into()))?;
    let email_from = std::env::var("EMAIL_FROM")
        .unwrap_or_else(|_| "Discord bot <discord-bot@yourdomain.com>".into());

    let cfg = config::Config::load(&cli.config)?;
    tracing::info!(config = %cli.config, "loaded configuration");

    let store = store::Store::open(&cfg.storage.dsn)?;
    let mailer = mailer::Mailer::new(resend_api_key, email_from);
    let verify = verify::VerifyService::new(store.clone(), mailer.clone());

    let backup_dir = cfg.storage.backup_dir.clone();
    let framework = bot::build(store, mailer, verify, cli.debug, backup_dir);

    let mut client = Client::builder(discord_token, bot::intents())
        .framework(framework)
        .await?;

    // Graceful shutdown on SIGINT/SIGTERM (Go: signal.Notify → close session).
    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut int = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        tokio::select! {
            _ = term.recv() => tracing::info!("received SIGTERM"),
            _ = int.recv() => tracing::info!("received SIGINT"),
        }
        tracing::info!("shutting down...");
        shard_manager.shutdown_all().await;
    });

    client.start_autosharded().await?;
    Ok(())
}
