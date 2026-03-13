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

    // Phase 1: Install packages
    if !args.skip_packages {
        println!("{}", bold.apply_to("Phase 1: Packages"));
        let total_packages: usize = modules.iter().map(|m| m.manifest.packages.len()).sum();
        let pb = ProgressBar::new(total_packages as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  [{bar:30}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=> "),
        );

        for module in &modules {
            for pkg in &module.manifest.packages {
                pb.set_message(pkg.name.clone());
                let status = pkg.check_status();
                if !status.installed {
                    if let Err(e) = pkg.install(ctx.dry_run) {
                        pb.println(format!("  ✗ {} — {e}", pkg.name));
                    }
                }
                pb.inc(1);
            }
        }
        pb.finish_and_clear();
        println!("  {green} {total_packages} package(s) checked", green = green.apply_to("✓"));
        println!();
    }

    // Phase 2: Deploy configs
    if !args.skip_configs {
        println!("{}", bold.apply_to("Phase 2: Configs"));
        let link_args = super::link::LinkArgs { module: None };
        super::link::run_link(link_args, ctx).await?;
        println!();
    }

    // Phase 3: Verify
    println!("{}", bold.apply_to("Phase 3: Verify"));
    let status_args = super::status::StatusArgs {
        json: false,
        module: None,
    };
    super::status::run(status_args, ctx).await?;

    println!();
    println!("{} Bootstrap complete", green.apply_to("✓"));

    Ok(())
}
