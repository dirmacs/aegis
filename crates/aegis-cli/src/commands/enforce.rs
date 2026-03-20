use anyhow::Result;
use console::Style;

use aegis_core::enforce::{self, Action};

use super::Context;

#[derive(clap::Args)]
pub struct EnforceArgs {
    /// Actually apply the remediation (default: show plan only)
    #[arg(long)]
    pub apply: bool,
    /// Filter to a specific module
    #[arg(long)]
    pub module: Option<String>,
}

pub async fn run(args: EnforceArgs, ctx: &Context) -> Result<()> {
    let (manifest, manifest_dir) = ctx.load_manifest()?;
    let modules = ctx.load_modules(&manifest, &manifest_dir, args.module.as_deref())?;

    let bold = Style::new().bold();
    let green = Style::new().green().bold();
    let yellow = Style::new().yellow().bold();
    let red = Style::new().red().bold();
    let dim = Style::new().dim();

    let plan = enforce::plan_remediation(&modules, manifest.aegis.strategy)?;

    if plan.is_clean() {
        println!("{} System is in compliance — no actions needed", green.apply_to("✓"));
        return Ok(());
    }

    let (installs, deploys, repairs) = plan.summary();
    println!(
        "{} Remediation plan: {} install(s), {} deploy(s), {} repair(s)\n",
        bold.apply_to("▸"),
        installs, deploys, repairs,
    );

    for action in &plan.actions {
        match action {
            Action::InstallPackage { module, package, method } => {
                println!(
                    "  {} {} → install {} {}",
                    yellow.apply_to("⚡"),
                    dim.apply_to(module),
                    bold.apply_to(package),
                    dim.apply_to(format!("({})", method.to_lowercase())),
                );
            }
            Action::DeployConfig { module, target, .. } => {
                println!(
                    "  {} {} → deploy {}",
                    yellow.apply_to("+"),
                    dim.apply_to(module),
                    bold.apply_to(target.display()),
                );
            }
            Action::RepairDrift { module, target, .. } => {
                println!(
                    "  {} {} → repair {}",
                    red.apply_to("~"),
                    dim.apply_to(module),
                    bold.apply_to(target.display()),
                );
            }
        }
    }

    if !args.apply {
        println!("\n{}", dim.apply_to("Run with --apply to execute this plan"));
        return Ok(());
    }

    println!();
    let (ok, fail) = enforce::apply_remediation(&plan, &modules, ctx.dry_run)?;

    if fail == 0 {
        println!("\n{} Enforcement complete — {ok} action(s) applied", green.apply_to("✓"));
    } else {
        println!(
            "\n{} Enforcement finished — {ok} ok, {fail} failed",
            red.apply_to("!"),
        );
    }

    Ok(())
}
