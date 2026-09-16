use thiserror::Error;

/// Top-level error type for the application.
///
/// Each subsystem contributes a variant; later sessions add `Store`, `Http`,
/// `Verify`, `Backup`, and `Discord`.
#[derive(Debug, Error)]
pub enum Error {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("shutdown signal received")]
    Shutdown,
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
