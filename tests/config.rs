//! Integration tests for the public config API.

use multipurpose_discord_bot::{config::Config, error::ConfigError};

fn write_temp_config(content: &str) -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().expect("create temp file");
    std::fs::write(file.path(), content).expect("write temp config");
    file
}

const EXAMPLE_CONFIG: &str = r#"storage:
  dsn: "./data/verifier.db"
  backup_dir: "./backups"
"#;

#[test]
fn load_round_trips_storage_config() {
    let file = write_temp_config(EXAMPLE_CONFIG);
    let cfg = Config::load(file.path()).expect("load should succeed");
    assert_eq!(cfg.storage.dsn, "./data/verifier.db");
    assert_eq!(cfg.storage.backup_dir, "./backups");
}

#[test]
fn load_missing_file_yields_io_error() {
    let err = Config::load("/nonexistent/config.yml").expect_err("must fail");
    assert!(matches!(err, ConfigError::Io(_)));
}

#[test]
fn load_empty_yaml_applies_defaults() {
    let file = write_temp_config("# empty\n");
    let cfg = Config::load(file.path()).expect("load should succeed");
    assert_eq!(cfg.storage.dsn, "./data/verifier.db");
    assert_eq!(cfg.storage.backup_dir, "./backups");
}
