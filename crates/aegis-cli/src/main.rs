mod commands;
mod output;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "aegis",
    about = "Aegis — dirmacs system configuration manager",
    version,
    propagate_version = true
)]
struct Cli {
    /// Path to aegis.toml manifest
    #[arg(long, global = true)]
    config: Option<String>,

    /// Active profile override
    #[arg(long, global = true)]
    profile: Option<String>,

    /// Show what would be done without making changes
    #[arg(long, global = true)]
    dry_run: bool,

    /// Enable verbose logging
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Initialize a new aegis.toml manifest
    Init(commands::init::InitArgs),
    /// Full system setup: install packages, deploy configs, verify
    Bootstrap(commands::bootstrap::BootstrapArgs),
    /// Capture live system state back into managed configs
    Sync(commands::sync::SyncArgs),
    /// Show drift between managed and live configs
    Diff(commands::diff::DiffArgs),
    /// Health check: what's installed, missing, version mismatches
    Status(commands::status::StatusArgs),
    /// Deploy managed configs to their target locations
    Link(commands::link::LinkArgs),
    /// Remove deployed managed configs
    Unlink(commands::link::UnlinkArgs),
    /// Generate and manage opencode configurations
    Opencode(commands::opencode::OpencodeArgs),
    /// Manage the dirmacs toolchain (ares, daedra, thulp, eruka, lancor)
    Toolchain(commands::toolchain::ToolchainArgs),
    /// Overlay network management (CA, peers, WireGuard configs)
    Net(commands::net::NetArgs),
    /// Node inventory — discover, sync, and diff environments across the mesh
    Inventory(commands::inventory::InventoryArgs),
    /// List and inspect profiles
    Profile(commands::profile::ProfileArgs),
    /// Encrypted secrets vault (passwords, API keys, tokens)
    Secrets(commands::secrets::SecretsArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up tracing
    let filter = if cli.verbose {
        "debug"
    } else {
        "info"
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| filter.into()))
        .without_time()
        .with_target(false)
        .init();

    if cli.no_color {
        console::set_colors_enabled(false);
    }

    let ctx = commands::Context {
        config_path: cli.config,
        profile: cli.profile,
        dry_run: cli.dry_run,
        verbose: cli.verbose,
    };

    match cli.command {
        Commands::Init(args) => commands::init::run(args, &ctx).await,
        Commands::Bootstrap(args) => commands::bootstrap::run(args, &ctx).await,
        Commands::Sync(args) => commands::sync::run(args, &ctx).await,
        Commands::Diff(args) => commands::diff::run(args, &ctx).await,
        Commands::Status(args) => commands::status::run(args, &ctx).await,
        Commands::Link(args) => commands::link::run_link(args, &ctx).await,
        Commands::Unlink(args) => commands::link::run_unlink(args, &ctx).await,
        Commands::Opencode(args) => commands::opencode::run(args, &ctx).await,
        Commands::Toolchain(args) => commands::toolchain::run(args, &ctx).await,
        Commands::Net(args) => commands::net::run(args, &ctx).await,
        Commands::Inventory(args) => commands::inventory::run(args, &ctx).await,
        Commands::Profile(args) => commands::profile::run(args, &ctx).await,
        Commands::Secrets(args) => commands::secrets::run(args, &ctx).await,
    }
}
