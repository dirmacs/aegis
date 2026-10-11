//! aegis-serve — the aegis secrets keychain as a service.
//!
//! The vault is opened once at startup with a master password supplied through
//! the environment, then held in memory and served over HTTP. Agents
//! authenticate with per-agent bearer tokens; only `GET` on a single secret
//! route ever returns a value.
//!
//! Module map:
//!
//! * [`state`] — fail-closed startup configuration and the shared vault.
//! * [`auth`] — bearer-token parsing and verification.
//! * [`routes`] — the HTTP surface, including the accept loop.

pub mod auth;
pub mod routes;
pub mod state;

pub use state::{AppState, Config, ConfigError};
