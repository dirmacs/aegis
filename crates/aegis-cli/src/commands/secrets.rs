use std::io::{IsTerminal, Read};

use anyhow::{Context as _, Result, bail};
use clap::Subcommand;
use console::style;

use aegis_secrets::store::SecretStore;

#[derive(clap::Args)]
pub struct SecretsArgs {
    /// Path to vault file (default: ~/.config/aegis/vault.toml)
    #[arg(long)]
    vault: Option<String>,

    /// Vault master password for non-interactive use. Takes precedence over the
    /// `AEGIS_VAULT_PASSWORD` environment variable and piped stdin. Hidden from
    /// `--help` so the CLI never advertises the non-interactive fallback chain.
    #[arg(long, hide = true)]
    password: Option<String>,

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
    let vault_path = match args.vault {
        Some(p) => std::path::PathBuf::from(p),
        None => SecretStore::default_path()?,
    };

    // One TTY check governs every fallback: piped stdin may be consumed on a
    // non-TTY, while an interactive prompt is only ever offered to a real
    // terminal. This is what keeps automation from ever blocking on a prompt.
    let stdin_is_tty = std::io::stdin().is_terminal();

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
            Some("/tmp/secret".into()),
            Some("env-value".into()),
            false,
            stdin_not_read, // a real path must not touch stdin
            |p| {
                assert_eq!(p, "/tmp/secret");
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
}
