//! YAML configuration loader — non-secret settings only.
//!
//! Secrets (`DISCORD_TOKEN`, `RESEND_API_KEY`, `EMAIL_FROM`) are loaded from
//! `.env` in `main.rs` via `dotenvy`.

use crate::error::ConfigError;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub dsn: String,
    pub backup_dir: String,
}

impl Config {
    /// Read, default, and validate the configuration file (no env expansion).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        let mut cfg: Config = serde_yaml::from_str(&raw)?;
        if cfg.storage.dsn.is_empty() {
            cfg.storage.dsn = "./data/verifier.db".to_string();
        }
        if cfg.storage.backup_dir.is_empty() {
            cfg.storage.backup_dir = "./backups".to_string();
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp_config(content: &str) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("create temp file");
        std::fs::write(file.path(), content).expect("write temp config");
        file
    }

    #[test]
    fn load_plain_yaml_without_secrets() {
        let yaml = r#"storage:
  dsn: "./data/custom.db"
  backup_dir: "./my_backups"
"#;
        let file = write_temp_config(yaml);
        let cfg = Config::load(file.path()).expect("load should succeed");
        assert_eq!(cfg.storage.dsn, "./data/custom.db");
        assert_eq!(cfg.storage.backup_dir, "./my_backups");
    }

    #[test]
    fn load_applies_storage_defaults() {
        let yaml = r#"storage:
  dsn: ""
"#;
        let file = write_temp_config(yaml);
        let cfg = Config::load(file.path()).expect("load should succeed");
        assert_eq!(cfg.storage.dsn, "./data/verifier.db");
        assert_eq!(cfg.storage.backup_dir, "./backups");
    }

    #[test]
    fn load_without_storage_applies_defaults() {
        let yaml = "# empty config\n";
        let file = write_temp_config(yaml);
        let cfg = Config::load(file.path()).expect("load should succeed");
        assert_eq!(cfg.storage.dsn, "./data/verifier.db");
        assert_eq!(cfg.storage.backup_dir, "./backups");
    }

    #[test]
    fn load_missing_file_errors() {
        let err = Config::load("/nonexistent/config.yml").expect_err("missing file must fail");
        assert!(matches!(err, ConfigError::Io(_)));
    }
}
