use crate::store::StoreError;
use crate::verify::VerifyError;
use thiserror::Error;

/// Top-level error type for the application.
///
/// Each subsystem contributes a variant; `Backup` arrives in Session 8.
/// `serenity::Error` and `serde_yaml::Error` are boxed to keep the `Err`
/// variant small (clippy `result_large_err`).
#[derive(Debug, Error)]
pub enum Error {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("store error: {0}")]
    Store(#[from] StoreError),

    #[error("verification error: {0}")]
    Verify(#[from] VerifyError),

    #[error("discord error: {0}")]
    Discord(Box<serenity::Error>),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Message(String),

    #[error("yaml error: {0}")]
    Yaml(Box<serde_yaml::Error>),

    #[error("shutdown signal received")]
    Shutdown,
}

impl From<serenity::Error> for Error {
    fn from(e: serenity::Error) -> Self {
        Self::Discord(Box::new(e))
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(e: serde_yaml::Error) -> Self {
        Self::Yaml(Box::new(e))
    }
}

/// Configuration loading/validation errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required configuration field: {0}")]
    Missing(&'static str),

    #[error("i/o error reading configuration: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}
