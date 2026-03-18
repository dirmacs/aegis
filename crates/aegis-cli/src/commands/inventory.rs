use std::path::PathBuf;

use anyhow::Result;
use console::Style;

use aegis_core::inventory::{self, NodeInventory};
use aegis_net::config::NetworkManifest;

use super::Context;

#[derive(clap::Args)]
pub struct InventoryArgs {
    #[command(subcommand)]
    pub command: InventoryCommand,
}

#[derive(clap::Subcommand)]
pub enum InventoryCommand {
    /// Discover local environment and save inventory
    Discover(DiscoverArgs),
    /// Show local or remote inventory
    Show(ShowArgs),
    /// Diff inventories between two nodes
    Diff(DiffArgs),
    /// Push local inventory to a remote node
    Push(PushArgs),
    /// Pull inventory from a remote node
    Pull(PullArgs),
}

#[derive(clap::Args)]
pub struct DiscoverArgs {
    /// Node name (from aegis-net manifest)
    #[arg(long)]
    pub node: String,
    /// Overlay IP
    #[arg(long, default_value = "10.42.0.3")]
    pub ip: String,
    /// Output path
    #[arg(long, default_value = "inventory.toml")]
    pub output: String,
}

#[derive(clap::Args)]
pub struct ShowArgs {
    /// Path to inventory file
    #[arg(default_value = "inventory.toml")]
    pub path: String,
}

#[derive(clap::Args)]
pub struct DiffArgs {
    /// Local inventory path
    pub local: String,
    /// Remote inventory path
    pub remote: String,
}

#[derive(clap::Args)]
pub struct PushArgs {
    /// Local inventory file
    #[arg(default_value = "inventory.toml")]
    pub file: String,
    /// SSH user@host
    #[arg(long)]
    pub to: String,
    /// Remote path to save inventory
    #[arg(long, default_value = "/etc/aegis-net/inventory/")]
    pub remote_dir: String,
}

#[derive(clap::Args)]
pub struct PullArgs {
    /// SSH user@host
    #[arg(long)]
    pub from: String,
    /// Remote inventory path
    #[arg(long)]
    pub path: String,
    /// Local output path
    #[arg(long, default_value = "remote-inventory.toml")]
    pub output: String,
}

pub async fn run(args: InventoryArgs, ctx: &Context) -> Result<()> {
    match args.command {
        InventoryCommand::Discover(a) => discover(a, ctx).await,
        InventoryCommand::Show(a) => show(a).await,
        InventoryCommand::Diff(a) => diff(a).await,
        InventoryCommand::Push(a) => push(a, ctx).await,
        InventoryCommand::Pull(a) => pull(a).await,
    }
}

async fn discover(args: DiscoverArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();

    if ctx.dry_run {
        println!("  [dry-run] Would discover environment for node '{}'", args.node);
        return Ok(());
    }

    println!("Discovering local environment...");
    let inv = inventory::discover_local(&args.node, &args.ip)?;

    let path = PathBuf::from(&args.output);
    inv.save(&path)?;

    println!("  {} Node: {} ({})", green.apply_to("✓"), inv.node, inv.overlay_ip);
    println!("  {} System: {} {} {}", green.apply_to("✓"), inv.system.hostname, inv.system.os, inv.system.arch);
    println!("  {} Tools: {} found", green.apply_to("✓"), inv.tools.len());
    println!("  {} Repos: {} found", green.apply_to("✓"), inv.repos.len());
    println!("  {} Saved: {}", green.apply_to("✓"), path.display());

    Ok(())
}

async fn show(args: ShowArgs) -> Result<()> {
    let inv = NodeInventory::load(&PathBuf::from(&args.path))?;
    let bold = Style::new().bold();
    let dim = Style::new().dim();

    println!("{}", bold.apply_to(format!("Node: {} ({})", inv.node, inv.overlay_ip)));
    println!("  {} {} {}", inv.system.hostname, inv.system.os, inv.system.arch);
    println!("  Snapshot: {}", inv.timestamp);
    println!();

    println!("{}", bold.apply_to("Tools:"));
    for t in &inv.tools {
        println!("  {:<20} {}", t.name, dim.apply_to(&t.path));
    }

    println!();
    println!("{}", bold.apply_to("Repos:"));
    for r in &inv.repos {
        println!("  {:<20} {}", r.name, dim.apply_to(&r.path));
    }

    if !inv.services.is_empty() {
        println!();
        println!("{}", bold.apply_to("Services:"));
        for s in &inv.services {
            let port = s.port.map(|p| format!(":{}", p)).unwrap_or_default();
            println!("  {:<20} {}{}", s.name, s.status, port);
        }
    }

    if !inv.models.is_empty() {
        println!();
        println!("{}", bold.apply_to("Models:"));
        for m in &inv.models {
            let gb = m.size_bytes as f64 / 1_073_741_824.0;
            println!("  {:<40} {:.1} GB", m.name, gb);
        }
    }

    Ok(())
}

async fn diff(args: DiffArgs) -> Result<()> {
    let local = NodeInventory::load(&PathBuf::from(&args.local))?;
    let remote = NodeInventory::load(&PathBuf::from(&args.remote))?;
    let d = local.diff(&remote);
    inventory::print_diff(&d);
    Ok(())
}

async fn push(args: PushArgs, ctx: &Context) -> Result<()> {
    let green = Style::new().green().bold();
    let inv = NodeInventory::load(&PathBuf::from(&args.file))?;

    let remote_path = format!("{}/{}.toml", args.remote_dir, inv.node);

    if ctx.dry_run {
        println!("  [dry-run] Would push {} → {}:{}", args.file, args.to, remote_path);
        return Ok(());
    }

    // SCP the file
    let status = std::process::Command::new("scp")
        .args([
            "-o", "ConnectTimeout=10",
            &args.file,
            &format!("{}:{}", args.to, remote_path),
        ])
        .status()?;

    if status.success() {
        println!("  {} Pushed {} → {}:{}", green.apply_to("✓"), inv.node, args.to, remote_path);
    } else {
        anyhow::bail!("SCP failed");
    }

    Ok(())
}

async fn pull(args: PullArgs) -> Result<()> {
    let green = Style::new().green().bold();

    let parts: Vec<&str> = args.from.split('@').collect();
    let (user, host) = if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        ("root", parts[0])
    };

    let inv = inventory::pull_remote(user, host, &args.path)?;
    let output = PathBuf::from(&args.output);
    inv.save(&output)?;

    println!("  {} Pulled {} ({}) → {}", green.apply_to("✓"), inv.node, inv.overlay_ip, output.display());
    Ok(())
}
