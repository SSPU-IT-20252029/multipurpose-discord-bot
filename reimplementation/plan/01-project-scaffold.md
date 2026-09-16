# Session 1 — Project Scaffold & Configuration

> **Goal:** create a compilable Cargo project with CLI flags, YAML config loader (env-var
> substitution), unified error type, and tracing-based logging. No Discord functionality yet.
>
> **Parity target:** Go `cmd/bot/main.go` (flags + bootstrap) and `internal/config/config.go`.

---

## 1.1 Initialize the project

```bash
cd /home/adamix/Rust/multipurpose-discord-bot
cargo init --name multipurpose-discord-bot
mkdir -p src/bot tests
```

Set `edition = "2024"` in `Cargo.toml` (requires rustc ≥ 1.85).

## 1.2 `Cargo.toml`

```toml
[package]
name = "multipurpose-discord-bot"
version = "0.1.0"
edition = "2024"
description = "Rust reimplementation of the sspu-verifier Discord bot"
license = "AGPL-3.0"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# Discord (added here now; wired up in Session 5)
serenity = "0.12"
poise = "0.6"

# Database
rusqlite = { version = "0.32", features = ["bundled"] }

# Serialization / config
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"

# HTTP
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }

# CLI + env
clap = { version = "4", features = ["derive", "env"] }

# Crypto
sha2 = "0.10"
rand = "0.9"
subtle = "2"

# Parsing / data
regex = "1"
csv = "1"
chrono = { version = "0.4", features = ["serde"] }

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }

[dev-dependencies]
tempfile = "3"
httpmock = "0.7"   # used in Session 3 to mock the Resend API
```

> `reqwest` uses `rustls-tls` instead of `native-tls` to keep builds dependency-light and
> match Go's zero-CGO static build goal. `rusqlite/bundled` compiles SQLite from source, so no
> system `libsqlite3` is needed.

## 1.3 CLI flags (`clap`)

Parity with Go flags `-config <path>` (default `config.yml`) and `-debug` (bool).

```rust
// src/main.rs (bootstrap)
#[derive(clap::Parser, Debug)]
#[command(name = "multipurpose-discord-bot", about = "Multipurpose Discord bot")]
pub struct Cli {
    /// Path to the YAML configuration file.
    #[arg(long, default_value = "config.yml")]
    config: String,

    /// Enable verbose debug logging (gateway + interaction details).
    #[arg(long, default_value_t = false)]
    debug: bool,
}
```

**Divergence note:** Go used a single-dash flag syntax (`-config`, `-debug`). clap supports
single-dash long flags only via `#[arg(short)]`-style single chars; to accept `-config` we use
`#[arg(long)]` which yields `--config`. If exact CLI parity matters, add
`#[command(disable_help_flag = false)]` and document that `--config`/`--debug` are the canonical
forms. Add `// PARITY:` comment noting the difference.

## 1.4 Configuration module — `src/config.rs`

Mirrors `internal/config/config.go`:

1. Read raw file text.
2. Replace every `${VAR_NAME}` with the environment value *before* YAML parsing
   (Go used `regexp` + `os.Getenv`).
3. Parse into the config struct via `serde_yaml`.
4. Apply defaults, validate required fields.

```rust
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Config {
    pub discord: DiscordConfig,
    pub email: EmailConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordConfig {
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailConfig {
    pub api_key: String,
    pub from: String,          // e.g. "Discord bot <discord-bot@yourdomain.com>"
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct StorageConfig {
    pub dsn: String,            // default "./data/verifier.db"
    pub backup_dir: String,     // default "./backups"
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        let expanded = expand_env(&raw);
        let mut cfg: Config = serde_yaml::from_str(&expanded)?;

        if cfg.storage.dsn.is_empty() { cfg.storage.dsn = "./data/verifier.db".into(); }
        if cfg.storage.backup_dir.is_empty() { cfg.storage.backup_dir = "./backups".into(); }

        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.discord.token.is_empty() { return Err(ConfigError::Missing("discord.token")); }
        if self.email.api_key.is_empty() { return Err(ConfigError::Missing("email.api_key")); }
        if self.email.from.is_empty()   { return Err(ConfigError::Missing("email.from")); }
        Ok(())
    }
}

fn expand_env(input: &str) -> String {
    // Same semantics as Go: replace ${NAME}; unset vars become empty strings.
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}").unwrap());
    re.replace_all(input, |caps: &regex::Captures| {
        std::env::var(&caps[1]).unwrap_or_default()
    }).into_owned()
}
```

## 1.5 Error type — `src/error.rs`

A single unified error enum keeps downstream code ergonomic:

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config error: {0}")]          Config(#[from] ConfigError),
    #[error("database error: {0}")]        Store(#[from] rusqlite::Error),
    #[error("http error: {0}")]            Http(#[from] reqwest::Error),
    #[error("io error: {0}")]              Io(#[from] std::io::Error),
    #[error("yaml error: {0}")]            Yaml(#[from] serde_yaml::Error),
    #[error("serde error: {0}")]           Json(#[from] serde_json::Error),
    #[error("verification: {0}")]          Verify(#[from] verify::VerifyError),
    #[error("discord error: {0}")]         Discord(String),
    #[error("backup: {0}")]                Backup(String),
    #[error("shutdown signal received")]   Shutdown,
}
```

> Add `thiserror = "1"` to `Cargo.toml` — it is the modern, idiomatic error-derivation crate.

## 1.6 Logging

```rust
// src/main.rs
fn init_logging(debug: bool) {
    let filter = if debug {
        "debug,tower_http=debug,serenity=debug,poise=debug".to_string()
    } else {
        "info,serenity=warn,poise=warn".to_string()
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}
```

Session 5 will pass the `debug` flag through to `serenity`'s `framework::StandardFramework`
gateway debug logging for full parity.

## 1.7 `main.rs` skeleton (stubs, wired in later sessions)

```rust
mod config;
mod error;

#[tokio::main]
async fn main() -> Result<(), error::Error> {
    let cli = Cli::parse();
    init_logging(cli.debug);

    let cfg = config::Config::load(&cli.config)?;
    tracing::info!("loaded config from {}", cli.config);

    // Session 2: open store
    // Session 5: build bot + start gateway
    // Session 8: spawn backup scheduler

    Ok(())
}
```

---

## 1.8 Unit tests

| Test | Input | Expect |
|---|---|---|
| `expand_env` replaces `${DISCORD_TOKEN}` | `"token: ${DISCORD_TOKEN}"`, env set | value substituted |
| `expand_env` handles unset var | `${MISSING}` not in env | replaced with empty string |
| `expand_env` ignores `$` without braces | `"$NOTBRACED"` | untouched |
| `load` applies storage defaults | YAML with no `storage` section | `./data/verifier.db`, `./backups` |
| `validate` fails on missing discord token | empty `discord.token` | `ConfigError::Missing("discord.token")` |
| `validate` fails on missing email fields | empty `api_key` / `from` | corresponding `Missing` errors |

Tests write a temp YAML file via `tempfile`; env vars set with a scoped mutex guard (or
`serial_test`) to avoid cross-test races.

---

## 1.9 Session completion checklist

- [ ] `cargo build` succeeds with `edition = "2024"`
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] Config tests pass
- [ ] CLI flags `--config` / `--debug` accepted
- [ ] `.gitignore` contains `config.yml`, `data/`, `*.db`, `.env`, `/target`
