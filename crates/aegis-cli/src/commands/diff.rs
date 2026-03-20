use anyhow::Result;
use console::Style;

use aegis_core::diff::diff_files;
use aegis_core::manifest::LinkStrategy;

use super::Context;

#[derive(clap::Args)]
pub struct DiffArgs {
    /// Only diff a specific module
    #[arg(long)]
    pub module: Option<String>,
}

pub async fn run(args: DiffArgs, ctx: &Context) -> Result<()> {
    let (manifest, manifest_dir) = ctx.load_manifest()?;
    let modules = ctx.load_modules(&manifest, &manifest_dir, args.module.as_deref())?;

    let bold = Style::new().bold();
    let green = Style::new().green();
    let red = Style::new().red();
    let cyan = Style::new().cyan();

    let mut any_changes = false;

    for module in &modules {
        let mut module_has_changes = false;

        for cfg in &module.manifest.configs {
            if !cfg.applies_to_current_os() {
                continue;
            }
            let source = module.config_source_path(cfg);
            let target = module.config_target_path(cfg)?;
            let strategy = module.effective_strategy(cfg, manifest.aegis.strategy);

            // For symlinks, if the link is correct there's nothing to diff
            if strategy == LinkStrategy::Symlink {
                if target.is_symlink() {
                    if let Ok(link) = std::fs::read_link(&target) {
                        if link == source {
                            continue;
                        }
                    }
                }
                if !target.exists() {
                    if !module_has_changes {
                        println!("{}", bold.apply_to(format!("── {} ──", module.name)));
                        module_has_changes = true;
                    }
                    println!(
                        "  {} {} (not deployed)",
                        red.apply_to("✗"),
                        cfg.target
                    );
                    any_changes = true;
                    continue;
                }
            }

            if !source.exists() || !target.exists() {
                if !module_has_changes {
                    println!("{}", bold.apply_to(format!("── {} ──", module.name)));
                    module_has_changes = true;
                }
                if !source.exists() {
                    println!("  {} {} (source missing)", red.apply_to("!"), cfg.source);
                }
                if !target.exists() {
                    println!("  {} {} (target missing)", red.apply_to("✗"), cfg.target);
                }
                any_changes = true;
                continue;
            }

            match diff_files(&source, &target) {
                Ok(result) if result.has_changes => {
                    if !module_has_changes {
                        println!("{}", bold.apply_to(format!("── {} ──", module.name)));
                        module_has_changes = true;
                    }
                    println!("  {} {}", cyan.apply_to("~"), cfg.target);
                    for hunk in &result.hunks {
                        for line in &hunk.lines {
                            let styled = match line.tag {
                                aegis_core::diff::DiffTag::Add => {
                                    green.apply_to(format!("    +{}", line.content.trim_end()))
                                }
                                aegis_core::diff::DiffTag::Remove => {
                                    red.apply_to(format!("    -{}", line.content.trim_end()))
                                }
                                aegis_core::diff::DiffTag::Context => {
                                    Style::new()
                                        .dim()
                                        .apply_to(format!("     {}", line.content.trim_end()))
                                }
                            };
                            println!("{styled}");
                        }
                    }
                    any_changes = true;
                }
                Ok(_) => {} // No changes
                Err(e) => {
                    if !module_has_changes {
                        println!("{}", bold.apply_to(format!("── {} ──", module.name)));
                        module_has_changes = true;
                    }
                    println!(
                        "  {} {} (error: {e})",
                        red.apply_to("!"),
                        cfg.target
                    );
                    any_changes = true;
                }
            }
        }
    }

    if !any_changes {
        let green_bold = Style::new().green().bold();
        println!("{} No drift detected", green_bold.apply_to("✓"));
    }

    Ok(())
}
