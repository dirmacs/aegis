use std::process::Command;

use anyhow::{Context, Result, bail};
use tracing::info;

use crate::registry::ToolEntry;

/// Install a dirmacs tool via cargo.
pub async fn install_tool(tool: &ToolEntry, from_source: bool, dry_run: bool) -> Result<()> {
    if from_source {
        install_from_git(tool, dry_run)
    } else {
        install_from_crates_io(tool, dry_run)
    }
}

fn install_from_crates_io(tool: &ToolEntry, dry_run: bool) -> Result<()> {
    let args = vec!["install", &tool.cargo_crate];

    if dry_run {
        info!("[dry-run] would run: cargo {}", args.join(" "));
        return Ok(());
    }

    info!("installing {} from crates.io", tool.cargo_crate);
    let status = Command::new("cargo")
        .args(&args)
        .status()
        .with_context(|| format!("running cargo install {}", tool.cargo_crate))?;

    if !status.success() {
        bail!(
            "cargo install {} failed with exit code {:?}",
            tool.cargo_crate,
            status.code()
        );
    }
    Ok(())
}

fn install_from_git(tool: &ToolEntry, dry_run: bool) -> Result<()> {
    let args = vec!["install", "--git", &tool.git_repo];

    if dry_run {
        info!(
            "[dry-run] would run: cargo {}",
            args.join(" ")
        );
        return Ok(());
    }

    info!("installing {} from {}", tool.name, tool.git_repo);
    let status = Command::new("cargo")
        .args(&args)
        .status()
        .with_context(|| format!("running cargo install --git {}", tool.git_repo))?;

    if !status.success() {
        bail!(
            "cargo install --git {} failed",
            tool.git_repo
        );
    }
    Ok(())
}

/// Update a tool to the latest version.
pub async fn update_tool(tool: &ToolEntry, dry_run: bool) -> Result<()> {
    let args = vec!["install", &tool.cargo_crate, "--force"];

    if dry_run {
        info!("[dry-run] would run: cargo {}", args.join(" "));
        return Ok(());
    }

    info!("updating {} to latest", tool.cargo_crate);
    let status = Command::new("cargo")
        .args(&args)
        .status()
        .with_context(|| format!("running cargo install {} --force", tool.cargo_crate))?;

    if !status.success() {
        bail!("cargo install {} --force failed", tool.cargo_crate);
    }
    Ok(())
}
