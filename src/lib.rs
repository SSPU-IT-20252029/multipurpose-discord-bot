//! Multipurpose Discord bot — Rust reimplementation of the Go `sspu-verifier`.
//!
//! Module tree grows per session:
//! - Session 1: `config`, `error`
//! - Session 2: `store`
//! - Session 3: `i18n`, `mailer`
//! - Session 4: `verify`
//! - Session 8: `backup`
//! - Session 5-7: `bot` (poise command/component handlers)

pub mod bot;
pub mod config;
pub mod error;
pub mod i18n;
pub mod mailer;
pub mod store;
pub mod verify;
