use anyhow::Result;
use console::Style;

use aegis_toolchain::health;
use aegis_toolchain::installer;
use aegis_toolchain::registry;

use super::Context;

#[derive(clap::Args)]
pub struct ToolchainArgs {
    #[command(subcommand)]
    pub command: ToolchainCommand,
}

#[derive(clap::Subcommand)]
pub enum ToolchainCommand {
    /// Install dirmacs tools
    Install(InstallArgs),
    /// Show toolchain health and versions
    Status,
    /// Update all dirmacs tools to latest versions
    Update(UpdateArgs),
}

#[derive(clap::Args)]
pub struct InstallArgs {
    /// Specific tool to install (default: all)
    pub tool: Option<String>,
    /// Build from git source instead of crates.io
    #[arg(long)]
    pub from_source: bool,
}

#[derive(clap::Args)]
pub struct UpdateArgs {
    /// Specific tool to update (default: all)
    pub tool: Option<String>,
}

pub async fn run(args: ToolchainArgs, ctx: &Context) -> Result<()> {
    match args.command {
        ToolchainCommand::Install(install_args) => install(install_args, ctx).await,
        ToolchainCommand::Status => status(ctx).await,
        ToolchainCommand::Update(update_args) => update(update_args, ctx).await,
    }
}

async fn install(args: InstallArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();

    if let Some(ref name) = args.tool {
        let tool = registry::find_tool(name)
            .ok_or_else(|| anyhow::anyhow!("unknown tool: {name}"))?;
        installer::install_tool(&tool, args.from_source, ctx.dry_run).await?;
        println!("{} Installed {name}", green.apply_to("✓"));
    } else {
        let tools = registry::dirmacs_registry();
        for tool in &tools {
            if tool.binary_name.is_none() {
                continue; // Skip library-only crates
            }
            match installer::install_tool(tool, args.from_source, ctx.dry_run).await {
                Ok(()) => println!("{} Installed {}", green.apply_to("✓"), tool.name),
                Err(e) => {
                    let red = Style::new().red().bold();
                    println!("{} Failed to install {}: {e}", red.apply_to("✗"), tool.name);
                }
            }
        }
    }

    Ok(())
}

async fn status(_ctx: &Context) -> Result<()> {
    let bold = Style::new().bold();
    let green = Style::new().green();
    let red = Style::new().red();

    println!("{}", bold.apply_to("Dirmacs Toolchain Status"));
    println!();

    let statuses = health::check_all();
    for tool in &statuses {
        let icon = if tool.installed {
            green.apply_to("✓")
        } else {
            red.apply_to("✗")
        };
        let version = tool.version.as_deref().unwrap_or("not installed");
        let path = tool
            .binary_path
            .as_deref()
            .unwrap_or("—");
        println!("  {icon} {:<12} {version}", tool.name);
        if tool.installed {
            println!("    {}", Style::new().dim().apply_to(path));
        }
    }

    let installed = statuses.iter().filter(|t| t.installed).count();
    let total = statuses.len();
    println!();
    println!("  {installed}/{total} tools installed");

    Ok(())
}

async fn update(args: UpdateArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();

    if let Some(ref name) = args.tool {
        let tool = registry::find_tool(name)
            .ok_or_else(|| anyhow::anyhow!("unknown tool: {name}"))?;
        installer::update_tool(&tool, ctx.dry_run).await?;
        println!("{} Updated {name}", green.apply_to("✓"));
    } else {
        let tools = registry::dirmacs_registry();
        for tool in &tools {
            if tool.binary_name.is_none() {
                continue;
            }
            match installer::update_tool(tool, ctx.dry_run).await {
                Ok(()) => println!("{} Updated {}", green.apply_to("✓"), tool.name),
                Err(e) => {
                    let red = Style::new().red().bold();
                    println!("{} Failed to update {}: {e}", red.apply_to("✗"), tool.name);
                }
            }
        }
    }

    Ok(())
}
