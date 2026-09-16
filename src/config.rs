//! YAML configuration loader with `${ENV_VAR}` substitution.
//!
//! Parity target: Go `internal/config/config.go`.

use crate::error::ConfigError;
use regex::Regex;
use serde::Deserialize;
use std::path::Path;

/// Same semantics as the Go loader: sections may be omitted entirely, default
/// to zero values, and required fields are validated afterwards.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Config {
    #[serde(default)]
    pub discord: DiscordConfig,
    #[serde(default)]
    pub email: EmailConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "lowercase", default)]
pub struct DiscordConfig {
    pub token: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "lowercase", default)]
pub struct EmailConfig {
    pub api_key: String,
    pub from: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "lowercase", default)]
pub struct StorageConfig {
    pub dsn: String,
    pub backup_dir: String,
}

impl Config {
    /// Read, expand, parse, default, and validate the configuration file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        let expanded = expand_env(&raw);
        let mut cfg: Config = serde_yaml::from_str(&expanded)?;
        cfg.apply_defaults();
        cfg.validate()?;
        Ok(cfg)
    }

    fn apply_defaults(&mut self) {
        if self.storage.dsn.is_empty() {
            self.storage.dsn = "./data/verifier.db".to_string();
        }
        if self.storage.backup_dir.is_empty() {
            self.storage.backup_dir = "./backups".to_string();
        }
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.discord.token.is_empty() {
            return Err(ConfigError::Missing("discord.token"));
        }
        if self.email.api_key.is_empty() {
            return Err(ConfigError::Missing("email.api_key"));
        }
        if self.email.from.is_empty() {
            return Err(ConfigError::Missing("email.from"));
        }
        Ok(())
    }
}

static ENV_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

/// Replace every `${NAME}` occurrence with the value of environment variable
/// `NAME`; unset variables become empty strings. Bare `$` (no braces) is left
/// untouched. Parity with Go: `regexp.MustCompile(...).ReplaceAllFunc` +
/// `os.Getenv`.
fn expand_env(input: &str) -> String {
    let re = ENV_RE.get_or_init(|| {
        Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").expect("hardcoded env regex is valid")
    });
    re.replace_all(input, |caps: &regex::Captures| {
        std::env::var(&caps[1]).unwrap_or_default()
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Environment mutation is process-global; serialize tests that touch it.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // `set_var`/`remove_var` are `unsafe` on edition 2024.
    fn set_env(key: &str, value: &str) {
        unsafe { std::env::set_var(key, value) };
    }

    fn remove_env(key: &str) {
        unsafe { std::env::remove_var(key) };
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
"#;

    #[test]
    fn expand_env_replaces_set_variable() {
        let _guard = ENV_LOCK.lock();
        set_env("DISCORD_TOKEN", "abc123");
        assert_eq!(expand_env("token: ${DISCORD_TOKEN}"), "token: abc123");
    }

    #[test]
    fn expand_env_unset_becomes_empty() {
        let _guard = ENV_LOCK.lock();
        remove_env("MISSING_VAR_9F3C");
        assert_eq!(expand_env("token: ${MISSING_VAR_9F3C}"), "token: ");
    }

    #[test]
    fn expand_env_ignores_bare_dollar() {
        let _guard = ENV_LOCK.lock();
        set_env("NOT_BRACED", "x");
        assert_eq!(expand_env("cost: $NOT_BRACED"), "cost: $NOT_BRACED");
    }

    #[test]
    fn load_substitutes_env_vars() {
        let _guard = ENV_LOCK.lock();
        set_env("DISCORD_TOKEN", "tok");
        set_env("RESEND_API_KEY", "re_key");
        let file = write_temp_config(FULL_CONFIG);
        let cfg = Config::load(file.path()).expect("load should succeed");
        assert_eq!(cfg.discord.token, "tok");
        assert_eq!(cfg.email.api_key, "re_key");
        assert_eq!(cfg.email.from, "Discord bot <discord-bot@example.com>");
    }

    #[test]
    fn load_applies_storage_defaults() {
        let _guard = ENV_LOCK.lock();
        set_env("DISCORD_TOKEN", "tok");
        set_env("RESEND_API_KEY", "re_key");
        let yaml = r#"discord:
  token: ${DISCORD_TOKEN}

email:
  api_key: ${RESEND_API_KEY}
  from: "x@y.z"
"#;
        let file = write_temp_config(yaml);
        let cfg = Config::load(file.path()).expect("load should succeed");
        assert_eq!(cfg.storage.dsn, "./data/verifier.db");
        assert_eq!(cfg.storage.backup_dir, "./backups");
    }

    #[test]
    fn validate_rejects_missing_discord_token() {
        let yaml = r#"discord:
  token: ""

email:
  api_key: "re_key"
  from: "x@y.z"
"#;
        let file = write_temp_config(yaml);
        let err = Config::load(file.path()).expect_err("missing token must fail");
        assert!(matches!(err, ConfigError::Missing("discord.token")));
    }

    #[test]
    fn validate_rejects_missing_api_key() {
        let yaml = r#"discord:
  token: "tok"

email:
  api_key: ""
  from: "x@y.z"
"#;
        let file = write_temp_config(yaml);
        let err = Config::load(file.path()).expect_err("missing api_key must fail");
        assert!(matches!(err, ConfigError::Missing("email.api_key")));
    }

    #[test]
    fn validate_rejects_missing_from() {
        let yaml = r#"discord:
  token: "tok"

email:
  api_key: "re_key"
  from: ""
"#;
        let file = write_temp_config(yaml);
        let err = Config::load(file.path()).expect_err("missing from must fail");
        assert!(matches!(err, ConfigError::Missing("email.from")));
    }

    #[test]
    fn load_missing_file_errors() {
        let err = Config::load("/nonexistent/config.yml").expect_err("missing file must fail");
        assert!(matches!(err, ConfigError::Io(_)));
    }
}
