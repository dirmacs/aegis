use anyhow::Result;
use console::Style;

use super::Context;

#[derive(clap::Args)]
pub struct SyncArgs {
    /// Only sync a specific module
    #[arg(long)]
    pub module: Option<String>,
}

pub async fn run(args: SyncArgs, ctx: &Context) -> Result<()> {
    let (manifest, manifest_dir) = ctx.load_manifest()?;
    let modules = ctx.load_modules(&manifest, &manifest_dir, args.module.as_deref())?;

    let green = Style::new().green().bold();
    let _yellow = Style::new().yellow();
    let mut synced = 0;

    for module in &modules {
        for rule in &module.manifest.sync_rules {
            let live_path = {
                let expanded = shellexpand::tilde(&rule.live_path);
                std::path::PathBuf::from(expanded.as_ref())
            };
            let managed_path = module.base_path.join(&rule.managed_path);

            if !live_path.exists() {
                tracing::debug!("skipping sync — live path not found: {}", live_path.display());
                continue;
            }

            // For symlinked configs, sync is a no-op (changes go through the symlink)
            if managed_path.is_symlink() || live_path.is_symlink() {
                tracing::debug!("skipping sync — symlinked: {}", live_path.display());
                continue;
            }

            // Read both files and compare
            let live_content = std::fs::read(&live_path)?;
            let managed_exists = managed_path.exists();
            let needs_sync = if managed_exists {
                let managed_content = std::fs::read(&managed_path)?;
                live_content != managed_content
            } else {
                true
            };

            if needs_sync {
                if ctx.dry_run {
                    println!(
                        "[dry-run] would sync {} → {}",
                        live_path.display(),
                        managed_path.display()
                    );
                } else {
                    std::fs::write(&managed_path, &live_content)?;
                    tracing::info!(
                        "synced {} → {}",
                        live_path.display(),
                        managed_path.display()
                    );
                }
                synced += 1;
            }
        }
    }

    if synced > 0 {
        println!(
            "{} Synced {synced} config(s) from live system",
            green.apply_to("✓")
        );
    } else {
        println!(
            "{} Everything in sync",
            green.apply_to("✓")
        );
    }

    Ok(())
}
