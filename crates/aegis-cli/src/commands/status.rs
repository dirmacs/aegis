use anyhow::Result;
use console::Style;

use aegis_core::config::{ConfigStatus, check_config};
use aegis_core::variables::check_env_vars;

use super::Context;

#[derive(clap::Args)]
pub struct StatusArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
    /// Filter to a specific module
    #[arg(long)]
    pub module: Option<String>,
}

pub async fn run(args: StatusArgs, ctx: &Context) -> Result<()> {
    let (manifest, manifest_dir) = ctx.load_manifest()?;
    let modules = ctx.load_modules(&manifest, &manifest_dir, args.module.as_deref())?;

    let green = Style::new().green();
    let red = Style::new().red();
    let yellow = Style::new().yellow();
    let bold = Style::new().bold();

    // Profile info
    if let Some((name, profile)) = manifest.active_profile(ctx.profile.as_deref()) {
        println!(
            "{} Profile: {} — {}",
            bold.apply_to("▸"),
            bold.apply_to(name),
            profile.description.as_deref().unwrap_or("")
        );
        println!();
    }

    // Environment variables
    let env_statuses = check_env_vars(&manifest.variables);
    if !env_statuses.is_empty() {
        println!("{}", bold.apply_to("Environment Variables"));
        for status in &env_statuses {
            let icon = if status.set {
                green.apply_to("✓")
            } else {
                red.apply_to("✗")
            };
            println!(
                "  {icon} ${} ({})",
                status.env_var, status.variable_key
            );
        }
        println!();
    }

    // Module status
    println!("{}", bold.apply_to("Modules"));
    for module in &modules {
        println!("  {} {}", bold.apply_to("▸"), bold.apply_to(&module.name));

        // Package status
        for pkg in &module.manifest.packages {
            let status = pkg.check_status();
            let icon = if status.installed {
                green.apply_to("✓")
            } else {
                red.apply_to("✗")
            };
            let version = status.version.as_deref().unwrap_or("not found");
            println!("    {icon} {} — {version}", pkg.name);
        }

        // Config status
        for cfg in &module.manifest.configs {
            let source = module.config_source_path(cfg);
            let target = module.config_target_path(cfg)?;
            let strategy = module.effective_strategy(cfg, manifest.aegis.strategy);
            let status = check_config(&source, &target, strategy);

            let (icon, detail) = match status {
                ConfigStatus::Ok => (green.apply_to("✓"), "ok".to_string()),
                ConfigStatus::Missing => (red.apply_to("✗"), "missing".to_string()),
                ConfigStatus::Drifted(msg) => {
                    (yellow.apply_to("~"), format!("drifted: {msg}"))
                }
                ConfigStatus::Error(msg) => (red.apply_to("!"), format!("error: {msg}")),
            };
            println!("    {icon} {} → {} — {detail}", cfg.source, cfg.target);
        }
    }

    // Toolchain status
    println!();
    println!("{}", bold.apply_to("Dirmacs Toolchain"));
    let health = aegis_toolchain::health::check_all();
    for tool in &health {
        let icon = if tool.installed {
            green.apply_to("✓")
        } else {
            red.apply_to("✗")
        };
        let version = tool.version.as_deref().unwrap_or("not installed");
        println!("  {icon} {} — {version}", tool.name);
    }

    Ok(())
}
