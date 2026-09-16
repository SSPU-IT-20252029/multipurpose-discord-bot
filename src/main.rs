//! Binary entry point.
//!
//! Parity target: Go `cmd/bot/main.go` (flags + bootstrap). Sessions 2, 5, and
//! 8 wire in the store, Discord gateway, and backup scheduler.

use clap::Parser;
use multipurpose_discord_bot::{config, error::Error};

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
    let filter = if debug {
        "debug".to_string()
    } else {
        "info".to_string()
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    init_logging(cli.debug);

    // `_cfg` is consumed by later sessions (store open, bot build, scheduler).
    let _cfg = config::Config::load(&cli.config)?;
    tracing::info!(config = %cli.config, "loaded configuration");

    // Session 2: open store at cfg.storage.dsn
    // Session 5: build bot + start gateway
    // Session 8: spawn backup scheduler

    Ok(())
}
