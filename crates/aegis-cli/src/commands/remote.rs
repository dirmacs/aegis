use anyhow::{Result, bail};
use console::Style;

use aegis_core::ssh::SshTarget;

use super::Context;

#[derive(clap::Args)]
pub struct RemoteArgs {
    /// Target node name (from [nodes] in aegis.toml)
    pub node: String,
    /// Aegis subcommand to run on the remote node
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

pub async fn run(args: RemoteArgs, ctx: &Context) -> Result<()> {
    let (manifest, _) = ctx.load_manifest()?;
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

    if args.command.is_empty() {
        bail!("no command specified — usage: aegis remote <node> <command...>");
    }

    // Build the remote aegis command
    let aegis_env_path = node
        .aegis_env_path
        .as_deref()
        .unwrap_or("/opt/aegis-env");

    let subcommand = args.command.join(" ");
    let remote_cmd = format!(
        "cd {} && aegis --profile {} {}",
        shell_escape(aegis_env_path),
        shell_escape(&node.profile),
        subcommand,
    );

    println!(
        "{} {} → {} (profile: {})",
        bold.apply_to("▸"),
        bold.apply_to(&args.node),
        target.display(),
        bold.apply_to(&node.profile),
    );

    if ctx.dry_run {
        println!("  [dry-run] would run: ssh {} '{}'", target.display(), remote_cmd);
        return Ok(());
    }

    let status = target.exec_interactive(&remote_cmd)?;

    if status.success() {
        println!("\n{} Remote command completed on {}", green.apply_to("✓"), args.node);
    } else {
        println!(
            "\n{} Remote command failed on {} (exit {})",
            red.apply_to("✗"),
            args.node,
            status.code().unwrap_or(-1),
        );
    }

    Ok(())
}

fn shell_escape(s: &str) -> String {
    if s.contains(' ') || s.contains('\'') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}
