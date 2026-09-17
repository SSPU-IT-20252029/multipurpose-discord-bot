//! Binary entry point.
//!
//! Parity target: Go `cmd/bot/main.go` (flags, bootstrap, signal shutdown).

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

    let cfg = config::Config::load(&cli.config)?;
    tracing::info!(config = %cli.config, "loaded configuration");

    let store = store::Store::open(&cfg.storage.dsn)?;
    let mailer = mailer::Mailer::new(cfg.email.api_key.clone(), cfg.email.from.clone());
    let verify = verify::VerifyService::new(store.clone(), mailer.clone());

    let token = cfg.discord.token.clone();
    let backup_dir = cfg.storage.backup_dir.clone();
    let framework = bot::build(store, mailer, verify, cli.debug, backup_dir);

    let mut client = Client::builder(token, bot::intents())
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
