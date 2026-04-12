use anyhow::Result;
use console::Style;

use aegis_core::diff::diff_files;
use aegis_core::manifest::LinkStrategy;

use super::Context;

#[derive(clap::Args)]
pub struct WatchArgs {
    /// Seconds between drift checks (default: 60)
    #[arg(long, default_value = "60")]
    pub interval: u64,
    /// Auto-apply sync when drift is detected
    #[arg(long)]
    pub auto_sync: bool,
    /// Only watch a specific module
    #[arg(long)]
    pub module: Option<String>,
    /// Stop after this many iterations (0 = run forever, default: 0)
    #[arg(long, default_value = "0")]
    pub max_iterations: u64,
}

pub async fn run(args: WatchArgs, ctx: &Context) -> Result<()> {
    let bold = Style::new().bold();
    let green = Style::new().green().bold();
    let red = Style::new().red().bold();
    let yellow = Style::new().yellow();
    let dim = Style::new().dim();

    println!(
        "{} every {}s{}{}",
        bold.apply_to("Watching for config drift"),
        args.interval,
        if args.auto_sync { " [auto-sync on]" } else { "" },
        if args.max_iterations > 0 {
            format!(" [max {} iterations]", args.max_iterations)
        } else {
            String::new()
        }
    );
    println!("{}", dim.apply_to("Press Ctrl+C to stop."));
    println!();

    let mut iteration = 0u64;

    loop {
        iteration += 1;
        let now = {
            use std::time::{SystemTime, UNIX_EPOCH};
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("{:02}:{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
        };

        let (manifest, manifest_dir) = ctx.load_manifest()?;
        let modules =
            ctx.load_modules(&manifest, &manifest_dir, args.module.as_deref())?;

        let mut drifted_files: Vec<String> = Vec::new();

        for module in &modules {
            for cfg in &module.manifest.configs {
                if !cfg.applies_to_current_os() {
                    continue;
                }
                let source = module.config_source_path(cfg);
                let target = module.config_target_path(cfg)?;
                let strategy = module.effective_strategy(cfg, manifest.aegis.strategy);

                // Symlinks — check the link target is correct
                if strategy == LinkStrategy::Symlink {
                    if target.is_symlink() {
                        if let Ok(link) = std::fs::read_link(&target) {
                            if link == source {
                                continue; // link is correct
                            }
                        }
                    }
                    if !target.exists() {
                        drifted_files.push(format!("{}/{} (not deployed)", module.name, cfg.target));
                    } else {
                        drifted_files.push(format!("{}/{} (wrong link)", module.name, cfg.target));
                    }
                    continue;
                }

                // File copy — diff content
                if !source.exists() || !target.exists() {
                    drifted_files.push(format!("{}/{} (missing)", module.name, cfg.target));
                    continue;
                }

                if let Ok(result) = diff_files(&source, &target) {
                    if result.has_changes {
                        drifted_files.push(format!("{}/{}", module.name, cfg.target));
                    }
                }
            }
        }

        // Report
        if drifted_files.is_empty() {
            println!(
                "[{}] {}",
                dim.apply_to(now.to_string()),
                green.apply_to("✓ no drift detected"),
            );
        } else {
            println!(
                "[{}] {} ({} file(s) drifted):",
                dim.apply_to(now.to_string()),
                red.apply_to("✗ drift detected"),
                drifted_files.len(),
            );
            for f in &drifted_files {
                println!("    {} {}", yellow.apply_to("↕"), f);
            }

            // Auto-sync if requested
            if args.auto_sync && !ctx.dry_run {
                let sync_args = super::sync::SyncArgs {
                    module: args.module.clone(),
                };
                match super::sync::run(sync_args, ctx).await {
                    Ok(()) => println!("    {} auto-sync applied", green.apply_to("✓")),
                    Err(e) => println!("    {} auto-sync failed: {e}", red.apply_to("!")),
                }
            }
        }

        // Respect max_iterations
        if args.max_iterations > 0 && iteration >= args.max_iterations {
            println!(
                "{}",
                dim.apply_to(format!("Reached {iteration} iterations, stopping."))
            );
            break;
        }

        tokio::time::sleep(std::time::Duration::from_secs(args.interval)).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::WatchArgs;

    fn default_watch_args() -> WatchArgs {
        WatchArgs {
            interval: 60,
            auto_sync: false,
            module: None,
            max_iterations: 0,
        }
    }

    #[test]
    fn default_watch_args_are_sensible() {
        let args = default_watch_args();
        assert_eq!(args.interval, 60);
        assert!(!args.auto_sync);
        assert!(args.module.is_none());
        assert_eq!(args.max_iterations, 0);
    }

    #[test]
    fn watch_args_accept_custom_interval() {
        let args = WatchArgs {
            interval: 30,
            auto_sync: true,
            module: Some("nim".into()),
            max_iterations: 5,
        };
        assert_eq!(args.interval, 30);
        assert!(args.auto_sync);
        assert_eq!(args.module.as_deref(), Some("nim"));
        assert_eq!(args.max_iterations, 5);
    }
}
