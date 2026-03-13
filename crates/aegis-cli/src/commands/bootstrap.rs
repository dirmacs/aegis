use anyhow::Result;
use console::Style;
use indicatif::{ProgressBar, ProgressStyle};

use super::Context;

#[derive(clap::Args)]
pub struct BootstrapArgs {
    /// Skip package installation
    #[arg(long)]
    pub skip_packages: bool,
    /// Skip config deployment
    #[arg(long)]
    pub skip_configs: bool,
}

pub async fn run(args: BootstrapArgs, ctx: &Context) -> Result<()> {
    let (manifest, manifest_dir) = ctx.load_manifest()?;
    let modules = ctx.load_modules(&manifest, &manifest_dir, None)?;

    let bold = Style::new().bold();
    let green = Style::new().green().bold();
    let red = Style::new().red().bold();

    let profile_name = manifest
        .active_profile(ctx.profile.as_deref())
        .map(|(n, _)| n.to_string())
        .unwrap_or_else(|| "default".to_string());

    println!(
        "{} Bootstrapping with profile: {}",
        bold.apply_to("▸"),
        bold.apply_to(&profile_name)
    );
    println!();

    let mut pkg_ok = 0usize;
    let mut pkg_fail = 0usize;
    let mut config_ok = false;

    // Phase 1: Install packages
    if !args.skip_packages {
        println!("{}", bold.apply_to("Phase 1: Packages"));
        let total_packages: usize = modules.iter().map(|m| m.manifest.packages.len()).sum();
        let pb = ProgressBar::new(total_packages as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  [{bar:30}] {pos}/{len} {msg}")
                .expect("valid progress template")
                .progress_chars("=> "),
        );

        for module in &modules {
            for pkg in &module.manifest.packages {
                pb.set_message(pkg.name.clone());
                let status = pkg.check_status();
                if !status.installed {
                    if let Err(e) = pkg.install(ctx.dry_run) {
                        pb.println(format!("  ✗ {} — {e}", pkg.name));
                        pkg_fail += 1;
                    } else {
                        pkg_ok += 1;
                    }
                } else {
                    pkg_ok += 1;
                }
                pb.inc(1);
            }
        }
        pb.finish_and_clear();

        if pkg_fail > 0 {
            println!(
                "  {} {pkg_ok} ok, {pkg_fail} failed",
                red.apply_to("!")
            );
        } else {
            println!(
                "  {} {pkg_ok} package(s) checked",
                green.apply_to("✓")
            );
        }
        println!();
    }

    // Phase 2: Deploy configs
    if !args.skip_configs {
        println!("{}", bold.apply_to("Phase 2: Configs"));
        let link_args = super::link::LinkArgs { module: None };
        match super::link::run_link(link_args, ctx).await {
            Ok(()) => config_ok = true,
            Err(e) => {
                println!("  {} Config deployment failed: {e}", red.apply_to("✗"));
            }
        }
        println!();
    } else {
        config_ok = true;
    }

    // Phase 3: Verify
    println!("{}", bold.apply_to("Phase 3: Verify"));
    let status_args = super::status::StatusArgs {
        json: false,
        module: None,
    };
    super::status::run(status_args, ctx).await?;

    // Summary
    println!();
    if pkg_fail == 0 && config_ok {
        println!("{} Bootstrap complete", green.apply_to("✓"));
    } else {
        println!("{} Bootstrap finished with issues:", red.apply_to("!"));
        if pkg_fail > 0 {
            println!("  - {pkg_fail} package(s) failed to install");
        }
        if !config_ok {
            println!("  - Config deployment had errors");
        }
    }

    Ok(())
}
