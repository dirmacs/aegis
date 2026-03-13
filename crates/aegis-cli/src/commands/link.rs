use anyhow::Result;
use console::Style;

use aegis_core::config::{deploy_config, undeploy_config};
use aegis_core::manifest::LinkStrategy;
use aegis_core::module::HookEvent;
use aegis_core::template;
use aegis_core::variables::resolve_variables;

use super::Context;

#[derive(clap::Args)]
pub struct LinkArgs {
    /// Only link a specific module
    #[arg(long)]
    pub module: Option<String>,
}

#[derive(clap::Args)]
pub struct UnlinkArgs {
    /// Only unlink a specific module
    #[arg(long)]
    pub module: Option<String>,
}

pub async fn run_link(args: LinkArgs, ctx: &Context) -> Result<()> {
    let (manifest, manifest_dir) = ctx.load_manifest()?;
    let modules = ctx.load_modules(&manifest, &manifest_dir, args.module.as_deref())?;

    let profile = manifest.active_profile(ctx.profile.as_deref());
    let profile_vars = profile
        .map(|(_, p)| p.variables.clone())
        .unwrap_or_default();
    let variables = resolve_variables(&manifest.variables, &profile_vars)?;

    let green = Style::new().green().bold();
    let mut linked = 0;

    for module in &modules {
        // Run pre-link hooks
        for hook in module.hooks_for(HookEvent::PreLink) {
            if !ctx.dry_run {
                let status = std::process::Command::new("sh")
                    .args(["-c", &hook.command])
                    .status()?;
                if !status.success() {
                    tracing::warn!("pre-link hook failed: {}", hook.command);
                }
            }
        }

        for cfg in &module.manifest.configs {
            let source = module.config_source_path(cfg);
            let target = module.config_target_path(cfg)?;
            let strategy = module.effective_strategy(cfg, manifest.aegis.strategy);

            match strategy {
                LinkStrategy::Template => {
                    // Render the template to a temp file, then deploy as copy
                    let rendered = template::render_file(&source, &variables)?;
                    if ctx.dry_run {
                        println!(
                            "[dry-run] would render template {} → {}",
                            source.display(),
                            target.display()
                        );
                    } else {
                        if let Some(parent) = target.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::write(&target, rendered)?;
                        tracing::info!("rendered {} → {}", source.display(), target.display());
                    }
                }
                _ => {
                    deploy_config(&source, &target, strategy, ctx.dry_run)?;
                }
            }
            linked += 1;
        }

        // Run post-link hooks
        for hook in module.hooks_for(HookEvent::PostLink) {
            if !ctx.dry_run {
                let status = std::process::Command::new("sh")
                    .args(["-c", &hook.command])
                    .status()?;
                if !status.success() {
                    tracing::warn!("post-link hook failed: {}", hook.command);
                }
            }
        }
    }

    println!(
        "{} Linked {linked} config(s) across {} module(s)",
        green.apply_to("✓"),
        modules.len()
    );
    Ok(())
}

pub async fn run_unlink(args: UnlinkArgs, ctx: &Context) -> Result<()> {
    let (manifest, manifest_dir) = ctx.load_manifest()?;
    let modules = ctx.load_modules(&manifest, &manifest_dir, args.module.as_deref())?;

    let green = Style::new().green().bold();
    let mut unlinked = 0;

    for module in &modules {
        for hook in module.hooks_for(HookEvent::PreUnlink) {
            if !ctx.dry_run {
                let _ = std::process::Command::new("sh")
                    .args(["-c", &hook.command])
                    .status();
            }
        }

        for cfg in &module.manifest.configs {
            let target = module.config_target_path(cfg)?;
            undeploy_config(&target, ctx.dry_run)?;
            unlinked += 1;
        }

        for hook in module.hooks_for(HookEvent::PostUnlink) {
            if !ctx.dry_run {
                let _ = std::process::Command::new("sh")
                    .args(["-c", &hook.command])
                    .status();
            }
        }
    }

    println!(
        "{} Unlinked {unlinked} config(s) across {} module(s)",
        green.apply_to("✓"),
        modules.len()
    );
    Ok(())
}
