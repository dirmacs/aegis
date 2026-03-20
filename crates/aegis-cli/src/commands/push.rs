use anyhow::{Result, bail};
use console::Style;

use aegis_core::ssh::SshTarget;

use super::Context;

#[derive(clap::Args)]
pub struct PushArgs {
    /// Target node name (from [nodes] in aegis.toml)
    pub node: String,
    /// Run full bootstrap instead of just link after pushing
    #[arg(long)]
    pub bootstrap: bool,
    /// Only sync files, don't run aegis on remote
    #[arg(long)]
    pub sync_only: bool,
}

pub async fn run(args: PushArgs, ctx: &Context) -> Result<()> {
    let (manifest, manifest_dir) = ctx.load_manifest()?;
    let bold = Style::new().bold();
    let green = Style::new().green().bold();
    let red = Style::new().red().bold();

    let node = manifest
        .nodes
        .get(&args.node)
        .ok_or_else(|| anyhow::anyhow!("unknown node '{}' — check [nodes] in aegis.toml", args.node))?;

    let target = SshTarget {
        user: node.user.clone(),
        host: node.host.clone(),
        port: node.port,
        identity_file: node.identity_file.clone(),
    };

    let remote_path = node
        .aegis_env_path
        .as_deref()
        .unwrap_or("/opt/aegis-env");

    println!(
        "{} Pushing aegis-env to {} ({}:{})",
        bold.apply_to("▸"),
        bold.apply_to(&args.node),
        target.display(),
        remote_path,
    );

    if ctx.dry_run {
        println!("  [dry-run] would rsync {} → {}:{}", manifest_dir.display(), target.display(), remote_path);
        if !args.sync_only {
            let cmd = if args.bootstrap { "bootstrap" } else { "link" };
            println!("  [dry-run] would run: aegis --profile {} {}", node.profile, cmd);
        }
        return Ok(());
    }

    // Step 1: Ensure remote directory exists
    target.exec(&format!("mkdir -p {remote_path}"))?;

    // Step 2: Rsync aegis-env to remote
    let excludes = &[".git", "target", "node_modules", "__pycache__"];
    target.rsync_push(&manifest_dir, remote_path, excludes)?;
    println!("  {} Files synced", green.apply_to("✓"));

    if args.sync_only {
        println!("{} Sync complete (--sync-only)", green.apply_to("✓"));
        return Ok(());
    }

    // Step 3: Run aegis on remote
    let cmd = if args.bootstrap { "bootstrap" } else { "link" };
    println!(
        "\n{} Running aegis {} on {}...",
        bold.apply_to("▸"),
        bold.apply_to(cmd),
        args.node,
    );

    let remote_cmd = format!(
        "cd {} && aegis --profile {} {}",
        remote_path, node.profile, cmd,
    );

    let status = target.exec_interactive(&remote_cmd)?;

    if status.success() {
        println!("\n{} Push + {} complete on {}", green.apply_to("✓"), cmd, args.node);
    } else {
        bail!(
            "aegis {} failed on {} (exit {})",
            cmd,
            args.node,
            status.code().unwrap_or(-1),
        );
    }

    Ok(())
}
