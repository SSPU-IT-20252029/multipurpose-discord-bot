//! Integration tests for the public config API.
//!
//! `expand_env` internals are covered by unit tests inside `src/config.rs`.

use multipurpose_discord_bot::{config::Config, error::ConfigError};
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn set_env(key: &str, value: &str) {
    unsafe { std::env::set_var(key, value) };
}

fn write_temp_config(content: &str) -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().expect("create temp file");
    std::fs::write(file.path(), content).expect("write temp config");
    file
}

const FULL_CONFIG: &str = r#"discord:
  token: ${DISCORD_TOKEN}

email:
  api_key: ${RESEND_API_KEY}
  from: "Discord bot <discord-bot@example.com>"

storage:
  dsn: "./data/verifier.db"
  backup_dir: "./backups"
"#;

#[test]
fn load_round_trips_full_config() {
    let _guard = ENV_LOCK.lock();
    set_env("DISCORD_TOKEN", "tok");
    set_env("RESEND_API_KEY", "re_key");
    let file = write_temp_config(FULL_CONFIG);
    let cfg = Config::load(file.path()).expect("load should succeed");
    assert_eq!(cfg.discord.token, "tok");
    assert_eq!(cfg.email.api_key, "re_key");
    assert_eq!(cfg.email.from, "Discord bot <discord-bot@example.com>");
    assert_eq!(cfg.storage.dsn, "./data/verifier.db");
    assert_eq!(cfg.storage.backup_dir, "./backups");
}

#[test]
fn load_missing_file_yields_io_error() {
    let err = Config::load("/nonexistent/config.yml").expect_err("must fail");
    assert!(matches!(err, ConfigError::Io(_)));
}

#[test]
fn load_empty_token_yields_missing_field_error() {
    let yaml = r#"discord:
  token: ""

email:
  api_key: "re_key"
  from: "x@y.z"
"#;
    let file = write_temp_config(yaml);
    let err = Config::load(file.path()).expect_err("must fail");
    assert!(matches!(err, ConfigError::Missing("discord.token")));
}
