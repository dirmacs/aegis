use anyhow::Result;
use console::Style;
use serde::Serialize;

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

#[derive(Serialize)]
struct StatusOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    profile: Option<ProfileStatus>,
    env_vars: Vec<EnvVarStatus>,
    modules: Vec<ModuleStatus>,
    toolchain: Vec<ToolStatus>,
}

#[derive(Serialize)]
struct ProfileStatus {
    name: String,
    description: String,
}

#[derive(Serialize)]
struct EnvVarStatus {
    key: String,
    env_var: String,
    set: bool,
}

#[derive(Serialize)]
struct ModuleStatus {
    name: String,
    packages: Vec<PackageStatus>,
    configs: Vec<ConfigStatusEntry>,
}

#[derive(Serialize)]
struct PackageStatus {
    name: String,
    installed: bool,
    version: Option<String>,
}

#[derive(Serialize)]
struct ConfigStatusEntry {
    source: String,
    target: String,
    status: String,
    detail: Option<String>,
}

#[derive(Serialize)]
struct ToolStatus {
    name: String,
    installed: bool,
    version: Option<String>,
}

pub async fn run(args: StatusArgs, ctx: &Context) -> Result<()> {
    let (manifest, manifest_dir) = ctx.load_manifest()?;
    let modules = ctx.load_modules(&manifest, &manifest_dir, args.module.as_deref())?;

    if args.json {
        return run_json(&manifest, &modules, ctx);
    }

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

fn run_json(
    manifest: &aegis_core::manifest::Manifest,
    modules: &[aegis_core::module::Module],
    ctx: &Context,
) -> Result<()> {
    let profile_status = manifest
        .active_profile(ctx.profile.as_deref())
        .map(|(name, profile)| ProfileStatus {
            name: name.to_string(),
            description: profile.description.clone().unwrap_or_default(),
        });

    let env_statuses = check_env_vars(&manifest.variables);
    let env_vars: Vec<EnvVarStatus> = env_statuses
        .iter()
        .map(|s| EnvVarStatus {
            key: s.variable_key.clone(),
            env_var: s.env_var.clone(),
            set: s.set,
        })
        .collect();

    let mut module_statuses = Vec::new();
    for module in modules {
        let mut packages = Vec::new();
        for pkg in &module.manifest.packages {
            let status = pkg.check_status();
            packages.push(PackageStatus {
                name: pkg.name.clone(),
                installed: status.installed,
                version: status.version,
            });
        }

        let mut configs = Vec::new();
        for cfg in &module.manifest.configs {
            let source = module.config_source_path(cfg);
            let target = module.config_target_path(cfg)?;
            let strategy = module.effective_strategy(cfg, manifest.aegis.strategy);
            let status = check_config(&source, &target, strategy);

            let (status_str, detail) = match status {
                ConfigStatus::Ok => ("ok".to_string(), None),
                ConfigStatus::Missing => ("missing".to_string(), None),
                ConfigStatus::Drifted(msg) => ("drifted".to_string(), Some(msg)),
                ConfigStatus::Error(msg) => ("error".to_string(), Some(msg)),
            };

            configs.push(ConfigStatusEntry {
                source: cfg.source.clone(),
                target: cfg.target.clone(),
                status: status_str,
                detail,
            });
        }

        module_statuses.push(ModuleStatus {
            name: module.name.clone(),
            packages,
            configs,
        });
    }

    let health = aegis_toolchain::health::check_all();
    let toolchain: Vec<ToolStatus> = health
        .iter()
        .map(|t| ToolStatus {
            name: t.name.clone(),
            installed: t.installed,
            version: t.version.clone(),
        })
        .collect();

    let output = StatusOutput {
        profile: profile_status,
        env_vars,
        modules: module_statuses,
        toolchain,
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
