//! Startup configuration and shared vault state for the aegis secrets service.
//!
//! The service holds one [`SecretStore`] in memory for the process lifetime.
//! This module owns two things:
//!
//! * [`Config`] — validated, fail-closed startup configuration derived from the
//!   process environment. Every failure mode is an explicit enum variant so the
//!   reason can be reported without ever embedding the master password.
//! * [`AppState`] — the unlocked vault plus the agent token map, shared between
//!   request handlers behind a lock.
//!
//! # Fail-closed contract
//!
//! [`Config::from_env`] returns `Err` — and the caller binds no listener —
//! whenever [`ENV_VAULT_PASSWORD`] or [`ENV_TOKENS`] is missing, empty,
//! whitespace-only, or unparseable.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use aegis_secrets::store::SecretStore;
use anyhow::{Context, Result};
use thiserror::Error;

/// Environment variable naming the vault file. Defaults to [`DEFAULT_VAULT_PATH`].
pub const ENV_VAULT_PATH: &str = "AEGIS_VAULT_PATH";
/// Environment variable holding the vault master password. Required.
pub const ENV_VAULT_PASSWORD: &str = "AEGIS_VAULT_PASSWORD";
/// Environment variable holding `agent:token,agent:token` pairs. Required.
pub const ENV_TOKENS: &str = "AEGIS_TOKENS";
/// Environment variable naming the bind address. Defaults to [`DEFAULT_BIND_ADDR`].
pub const ENV_BIND: &str = "AEGIS_BIND";

/// Vault path used when `AEGIS_VAULT_PATH` is unset (`/data/vault.json`).
pub const DEFAULT_VAULT_PATH: &str = "/data/vault.json";
/// Bind address used when `AEGIS_BIND` is unset.
///
/// Loopback only, deliberately. The service serves secret values and accepts
/// bearer tokens over plain HTTP, so a wildcard default would put both on the
/// wire in the clear to anything that can route to the host. Widening beyond
/// the host is an explicit `AEGIS_BIND` opt-in, and if that is ever done it
/// must be paired with a transport that encrypts — the default is the safe
/// posture, not a claim that plaintext is acceptable.
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8477";

/// Prefix required on the `Authorization` header.
pub const BEARER_PREFIX: &str = "Bearer ";

/// Why the service refused to start. Variants never carry secret material.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error("{ENV_VAULT_PASSWORD} is not set")]
    MissingVaultPassword,
    #[error("{ENV_VAULT_PASSWORD} is empty or whitespace-only")]
    EmptyVaultPassword,
    #[error("{ENV_TOKENS} is not set")]
    MissingTokens,
    #[error("{ENV_TOKENS} is empty or whitespace-only")]
    EmptyTokens,
    #[error("{ENV_TOKENS} contains no usable 'agent:token' pair")]
    NoUsableTokens,
}

/// Validated startup configuration.
///
/// `Debug` never prints the master password or the token values, so the config
/// can be logged or included in an `Err` chain without leaking credentials.
#[derive(Clone)]
pub struct Config {
    pub vault_path: PathBuf,
    vault_password: String,
    pub tokens: HashMap<String, String>,
    pub bind: String,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("vault_path", &self.vault_path)
            .field("vault_password", &"[redacted]")
            .field(
                "tokens",
                &format_args!("{} agents (values redacted)", self.tokens.len()),
            )
            .field("bind", &self.bind)
            .finish()
    }
}

impl Config {
    /// Reads configuration from the process environment.
    ///
    /// This performs no I/O of its own beyond `std::env`, so a failure here
    /// happens before any socket exists.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_env_values(
            std::env::var(ENV_VAULT_PATH).ok(),
            std::env::var(ENV_VAULT_PASSWORD).ok(),
            std::env::var(ENV_TOKENS).ok(),
            std::env::var(ENV_BIND).ok(),
        )
    }

    /// Same as [`Config::from_env`] but with values supplied directly.
    ///
    /// The startup rules live here (not just inside `from_env`) so tests exercise
    /// the real validation path rather than a stand-in.
    pub fn from_env_values(
        vault_path: Option<String>,
        vault_password: Option<String>,
        tokens: Option<String>,
        bind: Option<String>,
    ) -> Result<Self, ConfigError> {
        let vault_password = vault_password.ok_or(ConfigError::MissingVaultPassword)?;
        let tokens = tokens.ok_or(ConfigError::MissingTokens)?;
        Self::from_parts(
            vault_path.unwrap_or_else(|| DEFAULT_VAULT_PATH.to_string()),
            &vault_password,
            &tokens,
            bind.unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string()),
        )
    }

    /// Builds a [`Config`] from concrete values, applying every fail-closed rule.
    pub fn from_parts(
        vault_path: impl Into<PathBuf>,
        vault_password: &str,
        raw_tokens: &str,
        bind: impl Into<String>,
    ) -> Result<Self, ConfigError> {
        if vault_password.trim().is_empty() {
            return Err(ConfigError::EmptyVaultPassword);
        }

        let tokens = crate::auth::parse_tokens(raw_tokens);
        if tokens.is_empty() {
            return Err(if raw_tokens.trim().is_empty() {
                ConfigError::EmptyTokens
            } else {
                ConfigError::NoUsableTokens
            });
        }

        Ok(Self {
            vault_path: vault_path.into(),
            vault_password: vault_password.to_string(),
            tokens,
            bind: bind.into(),
        })
    }

    /// The vault master password. Returned for the vault-open call only; callers
    /// must not log or propagate it.
    pub fn vault_password(&self) -> &str {
        &self.vault_password
    }

    /// Agent name that a token belongs to, if the token is known.
    pub fn agent_for_token(&self, token: &str) -> Option<&str> {
        self.tokens
            .iter()
            .find(|(_, value)| value.as_str() == token)
            .map(|(agent, _)| agent.as_str())
    }
}

/// State shared by every request handler.
pub struct AppState {
    vault: Mutex<SecretStore>,
    tokens: HashMap<String, String>,
}

impl AppState {
    /// Opens (creating when absent) and unlocks the vault described by `config`.
    ///
    /// # Errors
    ///
    /// Fails closed — with an error that names the vault path but never the
    /// password — when the vault cannot be read or parsed, or when the supplied
    /// password cannot decrypt the existing vault contents.
    pub fn new(config: &Config) -> Result<Self> {
        Self::with_vault(
            &config.vault_path,
            config.vault_password(),
            config.tokens.clone(),
        )
    }

    /// [`AppState::new`] with explicit inputs, for tests.
    pub fn with_vault(
        vault_path: &Path,
        vault_password: &str,
        tokens: HashMap<String, String>,
    ) -> Result<Self> {
        let store = open_vault(vault_path, vault_password)?;
        Ok(Self {
            vault: Mutex::new(store),
            tokens,
        })
    }

    /// Agent tokens, keyed by agent name.
    pub fn tokens(&self) -> &HashMap<String, String> {
        &self.tokens
    }

    /// Locks the vault, recovering a poisoned guard's contents rather than
    /// panicking on a request thread. All [`SecretStore`] operations are
    /// synchronous, so handlers never hold this guard across an `await`.
    pub fn vault(&self) -> MutexGuard<'_, SecretStore> {
        match self.vault.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl fmt::Debug for AppState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppState")
            .field("agents", &self.tokens.len())
            // Deliberately no vault listing: keys are metadata, but Debug output
            // can reach logs, so keep it to a count.
            .field("vault_locked", &true)
            .finish()
    }
}

/// Opens the vault at `path`, creating an empty one when the file is absent.
///
/// A missing file creates a fresh vault. Anything else that prevents the vault
/// from being read or decrypted with `master_password` is reported as an error
/// so the caller can exit without serving a request.
pub fn open_vault(path: &Path, master_password: &str) -> Result<SecretStore> {
    let store = SecretStore::open(path, master_password)
        .with_context(|| format!("failed to open vault at {}", path.display()))?;

    // `SecretStore::open` derives the key but does not prove it, so a vault that
    // exists with the wrong password only fails on first read. Probe it here:
    // if the vault is non-empty we must be able to decrypt at least one entry.
    if let Some((probe_key, _)) = store.list().first() {
        store
            .get(probe_key)
            .with_context(|| format!("vault password is incorrect for {}", path.display()))?;
    }

    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(agent, token)| ((*agent).to_string(), (*token).to_string()))
            .collect()
    }

    fn vault_config(dir: &tempfile::TempDir, name: &str) -> PathBuf {
        dir.path().join(name)
    }

    #[test]
    fn valid_parts_build_config() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = Config::from_parts(
            vault_config(&dir, "vault.toml"),
            "master-pw",
            "eruka:s3cret,ares:other",
            "127.0.0.1:1234",
        )
        .expect("valid config");
        assert_eq!(config.tokens.len(), 2);
        assert_eq!(
            config.tokens.get("eruka").map(String::as_str),
            Some("s3cret"),
            "token must be stored under its agent name"
        );
        assert_eq!(config.bind, "127.0.0.1:1234");
        assert_eq!(config.agent_for_token("other"), Some("ares"));
        assert_eq!(config.agent_for_token("nope"), None);
    }

    #[test]
    fn missing_env_values_are_distinct_errors() {
        // Password absent entirely.
        assert_eq!(
            Config::from_env_values(None, None, None, None).unwrap_err(),
            ConfigError::MissingVaultPassword,
            "absent password must be reported as absent, not as empty"
        );
        // Password present, tokens absent entirely.
        assert_eq!(
            Config::from_env_values(Some("v.toml".into()), Some("pw".into()), None, None)
                .unwrap_err(),
            ConfigError::MissingTokens,
            "absent tokens must be reported as absent, not as empty"
        );
    }

    #[test]
    fn empty_or_whitespace_password_refuses_to_start() {
        for password in ["", " ", "\t", "\n   "] {
            let err = Config::from_parts("v.toml", password, "eruka:s3cret", DEFAULT_BIND_ADDR)
                .unwrap_err();
            assert_eq!(
                err,
                ConfigError::EmptyVaultPassword,
                "password {password:?} must refuse to start"
            );
        }
    }

    #[test]
    fn empty_tokens_refuses_to_start() {
        for tokens in ["", "   ", "\t"] {
            let err = Config::from_parts("v.toml", "pw", tokens, DEFAULT_BIND_ADDR).unwrap_err();
            assert_eq!(
                err,
                ConfigError::EmptyTokens,
                "token set {tokens:?} must refuse to start"
            );
        }
    }

    #[test]
    fn unparseable_tokens_refuse_to_start() {
        for tokens in [
            "eruka",           // no separator
            ":secret",         // no agent name
            "eruka:",          // no token value
            " , , ",           // separators only
            "eruka,ares:s3cr", // first pair is garbage, second is fine: still valid
        ] {
            let err = Config::from_parts("v.toml", "pw", tokens, DEFAULT_BIND_ADDR);
            if tokens == "eruka,ares:s3cr" {
                assert!(
                    err.is_ok(),
                    "one parseable pair is enough once the set is non-empty"
                );
            } else {
                assert_eq!(
                    err.unwrap_err(),
                    ConfigError::NoUsableTokens,
                    "token set {tokens:?} must refuse to start"
                );
            }
        }
    }

    #[test]
    fn empty_tokens_never_yield_an_auth_map_that_accepts_anything() {
        // Even if a caller bypassed validation, parsing an empty token string
        // must produce a map with no entries, so no request can match.
        let map = crate::auth::parse_tokens("");
        assert!(
            map.is_empty(),
            "empty input must not produce any token entry"
        );
        let builder = hyper::http::HeaderMap::new();
        assert!(
            !crate::auth::is_authorized(&map, &builder),
            "an empty token map must reject every request"
        );
    }

    #[test]
    fn debug_never_prints_secrets() {
        let config =
            Config::from_parts("v.toml", "master-pw", "eruka:s3cret", "127.0.0.1:1").unwrap();
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("master-pw"), "password leaked in Debug");
        assert!(!rendered.contains("s3cret"), "token leaked in Debug");
        assert!(
            rendered.contains("1 agents"),
            "agent count should still be visible"
        );
    }

    #[test]
    fn config_defaults_match_the_declared_contract() {
        assert_eq!(DEFAULT_VAULT_PATH, "/data/vault.json");
        assert_eq!(DEFAULT_BIND_ADDR, "127.0.0.1:8477");
        // Parse it the way the service does, so the default cannot silently
        // drift to a wildcard address: a loopback default is a property of
        // the address, not of the string.
        let ip = DEFAULT_BIND_ADDR
            .rsplit_once(':')
            .expect("default bind has a :port")
            .0;
        let addr: std::net::IpAddr = ip.parse().expect("default bind has an IP");
        assert!(
            addr.is_loopback(),
            "default bind must be loopback, was {addr}"
        );
        assert_eq!(ENV_VAULT_PASSWORD, "AEGIS_VAULT_PASSWORD");
        assert_eq!(ENV_TOKENS, "AEGIS_TOKENS");
        assert_eq!(ENV_VAULT_PATH, "AEGIS_VAULT_PATH");
        assert_eq!(ENV_BIND, "AEGIS_BIND");
    }

    #[test]
    fn missing_vault_file_creates_fresh_vault_and_starts() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = vault_config(&dir, "absent.json");
        let tokens = token_map(&[("eruka", "s3cret")]);
        let state = AppState::with_vault(&path, "master-pw", tokens).expect("fresh vault");
        assert!(path.exists(), "a fresh vault must be created on disk");
        let guard = state.vault();
        assert!(guard.is_empty(), "a fresh vault starts empty");
    }

    #[test]
    fn corrupt_vault_fails_closed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = vault_config(&dir, "corrupt.json");
        std::fs::write(&path, "this is not a vault {{{").unwrap();
        let error = AppState::with_vault(&path, "master-pw", token_map(&[("eruka", "s3cret")]))
            .expect_err("corrupt vault must fail");
        let chain = format!("{error:#}");
        assert!(
            !chain.contains("master-pw"),
            "password must not appear in the error chain: {chain}"
        );
        assert!(chain.contains("failed to open vault"), "chain: {chain}");
    }

    #[test]
    fn wrong_password_fails_closed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = vault_config(&dir, "vault.json");
        {
            let mut store = SecretStore::open(&path, "right-password").unwrap();
            store.set("api-key", "value-1234", &["eruka"]).unwrap();
            store.save().unwrap();
        }
        let error =
            AppState::with_vault(&path, "wrong-password", token_map(&[("eruka", "s3cret")]))
                .expect_err("wrong password must fail");
        let chain = format!("{error:#}");
        assert!(
            !chain.contains("value-1234"),
            "secret value must not appear in the error chain: {chain}"
        );
        assert!(
            chain.contains("password is incorrect"),
            "chain should explain the password problem: {chain}"
        );
    }

    #[test]
    fn publication_of_state_does_not_leak_keys_in_debug() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = vault_config(&dir, "vault.json");
        let state = AppState::with_vault(&path, "master-pw", token_map(&[("eruka", "s3cret")]))
            .expect("vault");
        let rendered = format!("{state:?}");
        assert!(!rendered.contains("s3cret"));
        assert!(rendered.contains("agents"));
    }
}
