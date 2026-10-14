use std::io::{IsTerminal, Read};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use console::style;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use aegis_net::ca::{NodeCertificate, NodeKeypair, SigningKey};
use aegis_secrets::store::SecretStore;

#[derive(Args)]
pub struct SecretsArgs {
    /// Path to vault file (default: ~/.config/aegis/vault.toml)
    #[arg(long)]
    vault: Option<String>,

    /// Vault master password for non-interactive use. Takes precedence over the
    /// `AEGIS_VAULT_PASSWORD` environment variable and piped stdin. Hidden from
    /// `--help` so the CLI never advertises the non-interactive fallback chain.
    #[arg(long, hide = true)]
    password: Option<String>,

    /// Remote `aegis-serve` base URL (e.g., `http://127.0.0.1:8477`). Defaults
    /// to `AEGIS_SERVE_URL` when set; when provided, commands talk to the
    /// remote keychain service instead of opening a local vault file.
    #[arg(long)]
    url: Option<String>,

    /// Bearer token for authenticating to `aegis-serve` (defaults to `AEGIS_TOKEN`).
    #[arg(long, hide = true)]
    token: Option<String>,

    /// Optional `aegis-net` Ed25519 peer certificate path (`peer.cert`) sent in
    /// `X-Aegis-Cert` when talking to `aegis-serve` (defaults to `AEGIS_CERT_PATH`).
    #[arg(long)]
    cert: Option<String>,

    /// Optional `aegis-net` Ed25519 peer private key path (`peer.key`) used to
    /// sign `X-Aegis-Timestamp` + `X-Aegis-Signature` headers (defaults to `AEGIS_KEY_PATH`).
    #[arg(long)]
    key: Option<String>,

    #[command(subcommand)]
    command: SecretsCommand,
}

#[derive(Subcommand)]
enum SecretsCommand {
    /// Store a secret
    Set {
        /// Secret key name
        key: String,
        /// Secret value (omit for interactive prompt)
        value: Option<String>,
        /// Read the secret value from this file, or `-` for stdin. Takes
        /// precedence over the `AEGIS_SECRET_VALUE` environment variable.
        #[arg(long)]
        value_file: Option<String>,
        /// Tags for the secret
        #[arg(short, long)]
        tags: Vec<String>,
    },
    /// Retrieve a secret
    Get {
        /// Secret key name
        key: String,
    },
    /// List all stored secrets (keys + tags only, not values)
    List,
    /// Remove a secret
    Rm {
        /// Secret key name
        key: String,
    },
    /// Export a secret as an environment variable line (KEY=value)
    Export {
        /// Secret key name
        key: String,
        /// Environment variable name (defaults to uppercase key)
        #[arg(long)]
        env_name: Option<String>,
    },
}

pub async fn run(args: SecretsArgs, _ctx: &super::Context) -> Result<()> {
    // One TTY check governs every fallback: piped stdin may be consumed on a
    // non-TTY, while an interactive prompt is only ever offered to a real
    // terminal. This is what keeps automation from ever blocking on a prompt.
    let stdin_is_tty = std::io::stdin().is_terminal();

    let remote_url = args
        .url
        .or_else(|| std::env::var("AEGIS_SERVE_URL").ok())
        .filter(|u| !u.trim().is_empty());

    if let Some(base_url) = remote_url {
        let token = args
            .token
            .or_else(|| std::env::var("AEGIS_TOKEN").ok())
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "remote aegis-serve URL requires a bearer token: pass --token or set AEGIS_TOKEN"
                )
            })?;
        let cert_path = args
            .cert
            .or_else(|| std::env::var("AEGIS_CERT_PATH").ok())
            .filter(|c| !c.trim().is_empty());
        let cert_header = match cert_path.as_deref() {
            Some(path) => Some(
                NodeCertificate::load(Path::new(path))
                    .with_context(|| format!("loading peer certificate '{path}'"))?
                    .encode_header()
                    .context("encoding peer certificate header")?,
            ),
            None => None,
        };
        let key_path = args
            .key
            .or_else(|| std::env::var("AEGIS_KEY_PATH").ok())
            .filter(|k| !k.trim().is_empty());
        let signing_key = match key_path.as_deref() {
            Some(path) => Some(
                NodeKeypair::load_key(Path::new(path))
                    .with_context(|| format!("loading peer private key '{path}'"))?,
            ),
            None => None,
        };
        let client = RemoteSecretsClient {
            base_url,
            token,
            cert_header,
            signing_key,
        };
        return run_remote(&client, args.command, stdin_is_tty).await;
    }

    let vault_path = match args.vault {
        Some(p) => std::path::PathBuf::from(p),
        None => SecretStore::default_path()?,
    };

    let master_password = resolve_vault_password(
        args.password,
        std::env::var("AEGIS_VAULT_PASSWORD").ok(),
        stdin_is_tty,
        read_stdin_to_string,
        prompt_password,
    )?;

    let mut store = SecretStore::open(&vault_path, &master_password)?;

    match args.command {
        SecretsCommand::Set {
            key,
            value,
            value_file,
            tags,
        } => {
            let value = resolve_secret_value(
                value,
                value_file,
                std::env::var("AEGIS_SECRET_VALUE").ok(),
                stdin_is_tty,
                read_stdin_to_string,
                read_value_file,
                || prompt_value(&key),
            )?;
            let tag_refs: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
            store.set(&key, &value, &tag_refs)?;
            store.save()?;
            println!("{} Secret '{}' stored", style("✓").green().bold(), key);
        }

        SecretsCommand::Get { key } => {
            let value = store.get(&key)?;
            println!("{}", value);
        }

        SecretsCommand::List => {
            let secrets = store.list();
            if secrets.is_empty() {
                println!("{}", style("Vault is empty.").dim());
                return Ok(());
            }
            println!("{}", style(format!("{} secrets:", secrets.len())).bold());
            for (key, tags) in &secrets {
                let tag_str = if tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", tags.join(", "))
                };
                println!("  {} {}{}", style("•").dim(), key, style(tag_str).dim());
            }
        }

        SecretsCommand::Rm { key } => {
            store.remove(&key)?;
            store.save()?;
            println!("{} Secret '{}' removed", style("✓").green().bold(), key);
        }

        SecretsCommand::Export { key, env_name } => {
            let value = store.get(&key)?;
            let env = env_name.unwrap_or_else(|| key.to_uppercase().replace(['-', '.'], "_"));
            println!("{}={}", env, value);
        }
    }

    Ok(())
}

struct RemoteSecretsClient {
    base_url: String,
    token: String,
    cert_header: Option<String>,
    signing_key: Option<SigningKey>,
}

#[derive(serde::Deserialize)]
struct RemoteSecretListing {
    key: String,
    tags: Vec<String>,
}

#[derive(serde::Deserialize)]
struct RemoteListingResponse {
    secrets: Vec<RemoteSecretListing>,
}

impl RemoteSecretsClient {
    async fn request(
        &self,
        method: &str,
        path: &str,
        tags: &[String],
        body: Option<&str>,
    ) -> Result<(u16, String)> {
        let host_port = self
            .base_url
            .trim()
            .strip_prefix("http://")
            .unwrap_or(self.base_url.trim())
            .trim_end_matches('/');
        let host_port = host_port.split('/').next().unwrap_or(host_port);

        let mut stream =
            tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(host_port))
                .await
                .with_context(|| format!("connecting to aegis-serve at {host_port} timed out"))?
                .with_context(|| format!("connecting to aegis-serve at {host_port}"))?;

        let mut req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nConnection: close\r\nAuthorization: Bearer {}\r\n",
            self.token
        );
        if let Some(cert) = &self.cert_header {
            req.push_str(&format!("x-aegis-cert: {cert}\r\n"));
        }
        if let Some(signing_key) = &self.signing_key {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let sig = NodeCertificate::sign_request(signing_key, ts, method, path);
            req.push_str(&format!(
                "x-aegis-timestamp: {ts}\r\nx-aegis-signature: {sig}\r\n"
            ));
        }
        if !tags.is_empty() {
            req.push_str(&format!("x-aegis-tags: {}\r\n", tags.join(",")));
        }
        if let Some(payload) = body {
            req.push_str(&format!("Content-Length: {}\r\n", payload.len()));
        }
        req.push_str("\r\n");
        if let Some(payload) = body {
            req.push_str(payload);
        }

        stream
            .write_all(req.as_bytes())
            .await
            .context("writing request to aegis-serve")?;

        let mut raw = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut raw))
            .await
            .context("reading response from aegis-serve timed out")?
            .context("reading response from aegis-serve")?;

        let text = String::from_utf8_lossy(&raw).into_owned();
        let status_line = text.lines().next().unwrap_or_default();
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("malformed HTTP status line from aegis-serve"))?
            .parse()
            .context("parsing HTTP status code from aegis-serve")?;
        let resp_body = text
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default();
        Ok((status, resp_body))
    }

    async fn set(&self, key: &str, value: &str, tags: &[String]) -> Result<()> {
        let path = format!("/api/v1/secrets/{}", key.trim_start_matches('/'));
        let (status, body) = self.request("PUT", &path, tags, Some(value)).await?;
        if status != 200 {
            bail!("aegis-serve PUT {key} failed (HTTP {status}): {body}");
        }
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<String> {
        let path = format!("/api/v1/secrets/{}", key.trim_start_matches('/'));
        let (status, body) = self.request("GET", &path, &[], None).await?;
        if status == 404 {
            bail!("Secret '{key}' not found in aegis-serve");
        }
        if status != 200 {
            bail!("aegis-serve GET {key} failed (HTTP {status}): {body}");
        }
        Ok(body)
    }

    async fn list(&self) -> Result<Vec<RemoteSecretListing>> {
        let (status, body) = self.request("GET", "/api/v1/secrets", &[], None).await?;
        if status != 200 {
            bail!("aegis-serve LIST failed (HTTP {status}): {body}");
        }
        let parsed: RemoteListingResponse =
            serde_json::from_str(&body).context("parsing aegis-serve secrets list")?;
        Ok(parsed.secrets)
    }

    async fn remove(&self, key: &str) -> Result<()> {
        let path = format!("/api/v1/secrets/{}", key.trim_start_matches('/'));
        let (status, body) = self.request("DELETE", &path, &[], None).await?;
        if status == 404 {
            bail!("Secret '{key}' not found in aegis-serve");
        }
        if status != 200 {
            bail!("aegis-serve DELETE {key} failed (HTTP {status}): {body}");
        }
        Ok(())
    }
}

async fn run_remote(
    client: &RemoteSecretsClient,
    command: SecretsCommand,
    stdin_is_tty: bool,
) -> Result<()> {
    match command {
        SecretsCommand::Set {
            key,
            value,
            value_file,
            tags,
        } => {
            let resolved = resolve_secret_value(
                value,
                value_file,
                std::env::var("AEGIS_SECRET_VALUE").ok(),
                stdin_is_tty,
                read_stdin_to_string,
                read_value_file,
                || prompt_value(&key),
            )?;
            client.set(&key, &resolved, &tags).await?;
            println!("{} Secret '{}' stored", style("✓").green().bold(), key);
        }
        SecretsCommand::Get { key } => {
            let value = client.get(&key).await?;
            println!("{}", value);
        }
        SecretsCommand::List => {
            let mut secrets = client.list().await?;
            if secrets.is_empty() {
                println!("{}", style("Vault is empty.").dim());
                return Ok(());
            }
            secrets.sort_by(|a, b| a.key.cmp(&b.key));
            println!("{}", style(format!("{} secrets:", secrets.len())).bold());
            for entry in &secrets {
                let tag_str = if entry.tags.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", entry.tags.join(", "))
                };
                println!(
                    "  {} {}{}",
                    style("•").dim(),
                    entry.key,
                    style(tag_str).dim()
                );
            }
        }
        SecretsCommand::Rm { key } => {
            client.remove(&key).await?;
            println!("{} Secret '{}' removed", style("✓").green().bold(), key);
        }
        SecretsCommand::Export { key, env_name } => {
            let value = client.get(&key).await?;
            let env = env_name.unwrap_or_else(|| key.to_uppercase().replace(['-', '.'], "_"));
            println!("{}={}", env, value);
        }
    }
    Ok(())
}

/// Resolve the vault master password with this precedence, highest first:
/// 1. the `--password` flag,
/// 2. the `AEGIS_VAULT_PASSWORD` environment variable,
/// 3. stdin, read once with a single trailing newline trimmed — only when stdin
///    is *not* a TTY,
/// 4. an interactive prompt — only when stdin *is* a TTY (the last resort).
///
/// `read_stdin` runs only off-TTY and `prompt` only on-TTY, so a non-interactive
/// caller (piped/redirected stdin, cron, CI) that supplies none of the first two
/// sources gets a clear error instead of ever blocking on a prompt.
fn resolve_vault_password(
    flag: Option<String>,
    env: Option<String>,
    stdin_is_tty: bool,
    read_stdin: impl FnOnce() -> Result<String>,
    prompt: impl FnOnce() -> Result<String>,
) -> Result<String> {
    if let Some(pw) = provided(&flag) {
        return Ok(pw.to_string());
    }
    if let Some(pw) = provided(&env) {
        return Ok(pw.to_string());
    }
    if stdin_is_tty {
        return prompt();
    }
    let pw = trim_trailing_newline(&read_stdin()?).to_string();
    if pw.is_empty() {
        bail!(
            "no vault password available: pass --password, set AEGIS_VAULT_PASSWORD, \
             or pipe it on stdin (an interactive prompt is unavailable without a TTY)"
        );
    }
    Ok(pw)
}

/// Resolve a secret value with this precedence, highest first:
/// 1. the `--value` flag,
/// 2. the `--value-file` path (`-` means stdin),
/// 3. the `AEGIS_SECRET_VALUE` environment variable,
/// 4. an interactive prompt — only when stdin *is* a TTY (the last resort).
///
/// As with the password, `read_stdin` runs only off-TTY and `prompt` only on-TTY,
/// so automation that supplies none of the explicit sources errors instead of
/// blocking. A `--value-file -` on a TTY has no piped data to read and fails
/// loudly rather than hanging or silently falling through to a prompt.
fn resolve_secret_value(
    flag: Option<String>,
    value_file: Option<String>,
    env: Option<String>,
    stdin_is_tty: bool,
    read_stdin: impl FnOnce() -> Result<String>,
    read_file: impl FnOnce(&str) -> Result<String>,
    prompt: impl FnOnce() -> Result<String>,
) -> Result<String> {
    if let Some(v) = provided(&flag) {
        return Ok(v.to_string());
    }
    if let Some(path) = value_file.as_deref() {
        let raw = if path == "-" {
            if stdin_is_tty {
                bail!("--value-file - needs piped stdin, but stdin is a TTY");
            }
            read_stdin()?
        } else {
            read_file(path)?
        };
        // An explicit --value-file is a deliberate source: return its content,
        // stripping only the trailing newline an editor or `echo` would add.
        return Ok(trim_trailing_newline(&raw).to_string());
    }
    if let Some(v) = provided(&env) {
        return Ok(v.to_string());
    }
    if stdin_is_tty {
        return prompt();
    }
    let v = trim_trailing_newline(&read_stdin()?).to_string();
    if v.is_empty() {
        bail!(
            "no secret value available: pass --value, --value-file -, or set \
             AEGIS_SECRET_VALUE (an interactive prompt is unavailable without a TTY)"
        );
    }
    Ok(v)
}

/// Yield the contained string only when present and non-empty, so a blank flag or
/// empty env var is treated as "not supplied" and never silently unlocks the
/// vault with an empty secret.
fn provided(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|s| !s.is_empty())
}

/// Strip a single trailing newline (with an optional preceding carriage return),
/// leaving every other byte — including meaningful trailing whitespace — intact.
fn trim_trailing_newline(s: &str) -> &str {
    let s = s.strip_suffix('\n').unwrap_or(s);
    s.strip_suffix('\r').unwrap_or(s)
}

/// Read all of stdin verbatim (trimming belongs to the resolver). Only ever
/// invoked off-TTY, so it returns promptly at EOF and cannot wait on a prompt.
fn read_stdin_to_string() -> Result<String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("reading from stdin")?;
    Ok(buf)
}

/// Read a value file verbatim from disk.
fn read_value_file(path: &str) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading value file '{path}'"))
}

/// Interactive master-password prompt (hidden input). Reached only when stdin is
/// a TTY and no non-interactive source was supplied.
fn prompt_password() -> Result<String> {
    Ok(dialoguer::Password::new()
        .with_prompt("Vault password")
        .interact()?)
}

/// Interactive secret-value prompt (hidden input); the TTY-only last resort.
fn prompt_value(key: &str) -> Result<String> {
    Ok(dialoguer::Password::new()
        .with_prompt(format!("Value for '{key}'"))
        .interact()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stdin reader that fails the test if invoked — used to prove a source is
    /// NOT consulted once a higher-precedence source resolves (or on a TTY).
    fn stdin_not_read() -> Result<String> {
        panic!("stdin must not be read in this path");
    }

    /// A prompt that fails the test if invoked — used to prove the interactive
    /// path is never taken off-TTY. Encodes the never-hang guarantee: if the
    /// code ever reaches the prompt on a non-TTY, this panics and the test fails.
    fn prompt_not_called() -> Result<String> {
        panic!("the interactive prompt must not be shown in this path");
    }

    // ----- vault master password resolution -----

    #[test]
    fn password_flag_beats_env_and_reads_no_other_source() {
        let pw = resolve_vault_password(
            Some("from-flag".into()),
            Some("from-env".into()),
            false,
            stdin_not_read,
            prompt_not_called,
        )
        .expect("flag should resolve");
        assert_eq!(pw, "from-flag");
    }

    #[test]
    fn password_env_used_when_no_flag_and_not_a_tty() {
        let pw = resolve_vault_password(
            None,
            Some("from-env".into()),
            false,
            stdin_not_read,
            prompt_not_called,
        )
        .expect("env should resolve");
        assert_eq!(pw, "from-env");
    }

    #[test]
    fn password_read_from_stdin_when_piped() {
        let pw = resolve_vault_password(
            None,
            None,
            false,
            || Ok("from-stdin\n".into()),
            prompt_not_called,
        )
        .expect("piped stdin should resolve");
        assert_eq!(pw, "from-stdin", "trailing newline must be trimmed");
    }

    #[test]
    fn password_non_tty_without_any_source_errors_and_never_prompts() {
        // The headline guarantee: no flag/env and empty piped stdin on a non-TTY
        // must fail with a clear message, never block on a prompt.
        let err = resolve_vault_password(
            None,
            None,
            false,
            || Ok(String::new()),
            prompt_not_called, // panics if reached -> proves no prompt on non-TTY
        )
        .expect_err("no source on a non-TTY must error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("AEGIS_VAULT_PASSWORD") && msg.contains("stdin"),
            "error must explain the accepted sources, got: {msg}"
        );
    }

    #[test]
    fn password_blank_flag_and_env_fall_through_to_stdin() {
        let pw = resolve_vault_password(
            Some(String::new()),
            Some(String::new()),
            false,
            || Ok("from-stdin".into()),
            prompt_not_called,
        )
        .expect("blank flag/env should fall through to stdin");
        assert_eq!(pw, "from-stdin");
    }

    #[test]
    fn password_prompts_only_on_tty_and_does_not_read_stdin() {
        let pw = resolve_vault_password(
            None,
            None,
            true,           // a real terminal
            stdin_not_read, // panics if stdin is read on a TTY -> proves no read/hang
            || Ok("from-prompt".into()),
        )
        .expect("a TTY should prompt as the last resort");
        assert_eq!(pw, "from-prompt");
    }

    // ----- secret value resolution -----

    #[test]
    fn value_flag_beats_file_env_and_prompt() {
        let v = resolve_secret_value(
            Some("flag-value".into()),
            Some("-".into()),
            Some("env-value".into()),
            false,
            stdin_not_read,
            |_| panic!("no file should be read when --value is set"),
            prompt_not_called,
        )
        .expect("--value should resolve");
        assert_eq!(v, "flag-value");
    }

    #[test]
    fn value_file_dash_reads_stdin() {
        let v = resolve_secret_value(
            None,
            Some("-".into()),
            None,
            false,
            || Ok("stdin-value\n".into()),
            |_| panic!("a path reader must not be used for '-'"),
            prompt_not_called,
        )
        .expect("--value-file - should read stdin");
        assert_eq!(v, "stdin-value");
    }

    #[test]
    fn value_file_path_reads_file_and_trims_newline() {
        let v = resolve_secret_value(
            None,
            Some("/fixture/secret-file".into()),
            Some("env-value".into()),
            false,
            stdin_not_read, // a real path must not touch stdin
            |p| {
                assert_eq!(p, "/fixture/secret-file");
                Ok("file-value\n".into())
            },
            prompt_not_called,
        )
        .expect("--value-file should read the file");
        assert_eq!(v, "file-value");
    }

    #[test]
    fn value_env_used_when_no_flag_or_file_and_not_a_tty() {
        let v = resolve_secret_value(
            None,
            None,
            Some("env-value".into()),
            false,
            stdin_not_read,
            |_| panic!("no file expected"),
            prompt_not_called,
        )
        .expect("env should resolve");
        assert_eq!(v, "env-value");
    }

    #[test]
    fn value_non_tty_without_any_source_errors_and_never_prompts() {
        let err = resolve_secret_value(
            None,
            None,
            None,
            false,
            || Ok(String::new()),
            |_| panic!("no file expected"),
            prompt_not_called,
        )
        .expect_err("no source on a non-TTY must error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("AEGIS_SECRET_VALUE"),
            "error must name the env var, got: {msg}"
        );
    }

    #[test]
    fn value_file_dash_on_a_tty_errors_instead_of_hanging() {
        // `--value-file -` needs piped data; on a TTY it must fail loudly rather
        // than blocking on stdin or falling through to a prompt.
        let err = resolve_secret_value(
            None,
            Some("-".into()),
            None,
            true,
            stdin_not_read, // panics if stdin is read on a TTY
            |_| panic!("a TTY must not read a file for '-'"),
            prompt_not_called,
        )
        .expect_err("'-' on a TTY must error");
        let msg = format!("{err:#}");
        assert!(msg.contains("--value-file"), "got: {msg}");
    }

    #[test]
    fn value_prompts_only_on_tty_as_last_resort() {
        let v = resolve_secret_value(
            None,
            None,
            None,
            true,
            stdin_not_read,
            |_| panic!("no file expected"),
            || Ok("prompt-value".into()),
        )
        .expect("a TTY should prompt as the last resort");
        assert_eq!(v, "prompt-value");
    }

    #[test]
    fn value_file_precedes_env() {
        let v = resolve_secret_value(
            None,
            Some("-".into()),
            Some("env-value".into()),
            false,
            || Ok("stdin-value".into()),
            |_| panic!("unused"),
            prompt_not_called,
        )
        .expect("--value-file should beat the env var");
        assert_eq!(v, "stdin-value");
    }

    #[tokio::test]
    async fn remote_client_set_get_list_rm_and_export_roundtrip() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");

        let server = tokio::spawn(async move {
            for idx in 0..5 {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let mut buf = [0u8; 4096];
                let n = socket.read(&mut buf).await.expect("read");
                let req = String::from_utf8_lossy(&buf[..n]);
                assert!(req.contains("Authorization: Bearer test-token\r\n"));
                assert!(req.contains("x-aegis-cert: cert-b64\r\n"));
                assert!(req.contains("x-aegis-signature: "));

                let resp = match idx {
                    0 => {
                        assert!(req.starts_with("PUT /api/v1/secrets/env/app HTTP/1.1\r\n"));
                        assert!(req.contains("x-aegis-tags: prod,api\r\n"));
                        assert!(req.ends_with("\r\n\r\ns3cret"));
                        "HTTP/1.1 200 OK\r\nContent-Length: 15\r\n\r\n{\"status\":\"ok\"}"
                    }
                    1 => {
                        assert!(req.starts_with("GET /api/v1/secrets/env/app HTTP/1.1\r\n"));
                        "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\ns3cret"
                    }
                    2 => {
                        assert!(req.starts_with("GET /api/v1/secrets HTTP/1.1\r\n"));
                        let body = r#"{"secrets":[{"key":"env/app","tags":["prod","api"]}]}"#;
                        socket
                            .write_all(
                                format!(
                                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                                    body.len(),
                                    body
                                )
                                .as_bytes(),
                            )
                            .await
                            .expect("write list");
                        continue;
                    }
                    3 => {
                        assert!(req.starts_with("GET /api/v1/secrets/env/app HTTP/1.1\r\n"));
                        "HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\ns3cret"
                    }
                    _ => {
                        assert!(req.starts_with("DELETE /api/v1/secrets/env/app HTTP/1.1\r\n"));
                        "HTTP/1.1 200 OK\r\nContent-Length: 15\r\n\r\n{\"status\":\"ok\"}"
                    }
                };
                socket.write_all(resp.as_bytes()).await.expect("write");
            }
        });

        let (signing_key, _) = NodeKeypair::generate();
        let client = RemoteSecretsClient {
            base_url: format!("http://{addr}"),
            token: "test-token".to_string(),
            cert_header: Some("cert-b64".to_string()),
            signing_key: Some(signing_key),
        };

        run_remote(
            &client,
            SecretsCommand::Set {
                key: "env/app".to_string(),
                value: Some("s3cret".to_string()),
                value_file: None,
                tags: vec!["prod".to_string(), "api".to_string()],
            },
            false,
        )
        .await
        .expect("remote set");

        run_remote(
            &client,
            SecretsCommand::Get {
                key: "env/app".to_string(),
            },
            false,
        )
        .await
        .expect("remote get");

        run_remote(&client, SecretsCommand::List, false)
            .await
            .expect("remote list");

        run_remote(
            &client,
            SecretsCommand::Export {
                key: "env/app".to_string(),
                env_name: Some("APP_SECRET".to_string()),
            },
            false,
        )
        .await
        .expect("remote export");

        run_remote(
            &client,
            SecretsCommand::Rm {
                key: "env/app".to_string(),
            },
            false,
        )
        .await
        .expect("remote rm");

        server.await.expect("server join");
    }
}
