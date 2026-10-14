//! `aegis-serve` binary: validates startup configuration before binding a
//! socket, then serves the secrets API.
//!
//! Startup order is the fail-closed contract: configuration is validated first,
//! the vault is unlocked second, and the listener is bound only once both have
//! succeeded. Any failure before that point exits non-zero without ever
//! accepting a connection.

use std::process::ExitCode;
use std::sync::Arc;

use aegis_serve::routes::serve;
use aegis_serve::state::{AppState, Config};

fn main() -> ExitCode {
    tracing_subscriber::fmt::init();

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("aegis-serve: could not start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(run()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Safe to print: ConfigError variants and the vault-open context
            // deliberately exclude the master password and token values.
            eprintln!("aegis-serve: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    // Fail closed on missing/empty/whitespace AEGIS_VAULT_PASSWORD or
    // AEGIS_TOKENS, and on AEGIS_TOKENS with no parseable agent:token pair.
    let config = Config::from_env()?;
    let state = Arc::new(AppState::new(&config)?);

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    serve(listener, state).await
}
