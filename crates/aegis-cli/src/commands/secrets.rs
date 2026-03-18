use anyhow::Result;
use clap::Subcommand;
use console::style;

use aegis_secrets::store::SecretStore;

#[derive(clap::Args)]
pub struct SecretsArgs {
    /// Path to vault file (default: ~/.config/aegis/vault.toml)
    #[arg(long)]
    vault: Option<String>,

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

    // Prompt for master password
    let master_password = dialoguer::Password::new()
        .with_prompt("Vault password")
        .interact()?;

    let mut store = SecretStore::open(&vault_path, &master_password)?;

    match args.command {
        SecretsCommand::Set { key, value, tags } => {
            let value = match value {
                Some(v) => v,
                None => {
                    dialoguer::Password::new()
                        .with_prompt(format!("Value for '{}'", key))
                        .interact()?
                }
            };
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
            let env = env_name.unwrap_or_else(|| key.to_uppercase().replace('-', "_").replace('.', "_"));
            println!("{}={}", env, value);
        }
    }

    Ok(())
}
