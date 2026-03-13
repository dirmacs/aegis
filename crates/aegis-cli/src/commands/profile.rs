use anyhow::Result;
use console::Style;

use super::Context;

#[derive(clap::Args)]
pub struct ProfileArgs {
    #[command(subcommand)]
    pub command: ProfileCommand,
}

#[derive(clap::Subcommand)]
pub enum ProfileCommand {
    /// List all defined profiles
    List,
    /// Show details of a specific profile
    Show { name: String },
}

pub async fn run(args: ProfileArgs, ctx: &Context) -> Result<()> {
    match args.command {
        ProfileCommand::List => list(ctx).await,
        ProfileCommand::Show { name } => show(&name, ctx).await,
    }
}

async fn list(ctx: &Context) -> Result<()> {
    let (manifest, _) = ctx.load_manifest()?;
    let bold = Style::new().bold();
    let green = Style::new().green();

    let active = manifest.active_profile(ctx.profile.as_deref());
    let active_name = active.map(|(n, _)| n);

    println!("{}", bold.apply_to("Profiles"));
    for (name, profile) in &manifest.profiles {
        let marker = if Some(name.as_str()) == active_name {
            green.apply_to("● ")
        } else {
            Style::new().dim().apply_to("  ")
        };
        let desc = profile.description.as_deref().unwrap_or("");
        println!(
            "  {marker}{} — {desc} ({} modules)",
            bold.apply_to(name),
            profile.modules.len()
        );
    }

    Ok(())
}

async fn show(name: &str, ctx: &Context) -> Result<()> {
    let (manifest, _) = ctx.load_manifest()?;
    let bold = Style::new().bold();

    let profile = manifest
        .profiles
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("profile '{}' not found", name))?;

    println!("{} {}", bold.apply_to("Profile:"), name);
    if let Some(ref desc) = profile.description {
        println!("  {desc}");
    }
    println!();
    println!("{}", bold.apply_to("Modules:"));
    for module in &profile.modules {
        println!("  - {module}");
    }
    if !profile.variables.is_empty() {
        println!();
        println!("{}", bold.apply_to("Variables:"));
        for (key, val) in &profile.variables {
            println!("  {key} = {val}");
        }
    }

    Ok(())
}
