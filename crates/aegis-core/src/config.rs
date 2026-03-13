use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::manifest::LinkStrategy;

/// Mapping from a source config file to its target location.
#[derive(Debug, Deserialize, Serialize)]
pub struct ConfigMapping {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub strategy: Option<LinkStrategy>,
}

/// Deploy a config file using the given strategy.
pub fn deploy_config(
    source: &Path,
    target: &Path,
    strategy: LinkStrategy,
    dry_run: bool,
) -> Result<()> {
    if !source.exists() {
        bail!("source does not exist: {}", source.display());
    }

    if let Some(parent) = target.parent() {
        if !parent.exists() && !dry_run {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating parent dir {}", parent.display()))?;
        }
    }

    match strategy {
        LinkStrategy::Symlink => deploy_symlink(source, target, dry_run),
        LinkStrategy::Copy => deploy_copy(source, target, dry_run),
        LinkStrategy::Template => {
            // Template strategy is handled by the template engine before reaching here.
            // At this point the rendered content should be written as a regular file.
            deploy_copy(source, target, dry_run)
        }
    }
}

fn deploy_symlink(source: &Path, target: &Path, dry_run: bool) -> Result<()> {
    // If target already exists, check if it's already the right symlink
    if target.is_symlink() {
        let link_target = std::fs::read_link(target)
            .with_context(|| format!("reading symlink {}", target.display()))?;
        if link_target == source {
            info!("already linked: {} -> {}", target.display(), source.display());
            return Ok(());
        }
        // Remove existing symlink
        if !dry_run {
            std::fs::remove_file(target)
                .with_context(|| format!("removing existing symlink {}", target.display()))?;
        }
    } else if target.exists() {
        warn!(
            "target exists and is not a symlink: {} — backing up",
            target.display()
        );
        if !dry_run {
            let backup = target.with_extension("aegis-backup");
            std::fs::rename(target, &backup)
                .with_context(|| format!("backing up {}", target.display()))?;
            info!("backed up to {}", backup.display());
        }
    }

    if dry_run {
        info!("[dry-run] would symlink {} -> {}", target.display(), source.display());
    } else {
        #[cfg(unix)]
        std::os::unix::fs::symlink(source, target)
            .with_context(|| format!("symlinking {} -> {}", target.display(), source.display()))?;
        info!("linked {} -> {}", target.display(), source.display());
    }
    Ok(())
}

fn deploy_copy(source: &Path, target: &Path, dry_run: bool) -> Result<()> {
    if dry_run {
        info!("[dry-run] would copy {} -> {}", source.display(), target.display());
    } else {
        std::fs::copy(source, target)
            .with_context(|| format!("copying {} -> {}", source.display(), target.display()))?;
        info!("copied {} -> {}", source.display(), target.display());
    }
    Ok(())
}

/// Remove a deployed config (unlink symlink or delete copied file).
pub fn undeploy_config(target: &Path, dry_run: bool) -> Result<()> {
    if !target.exists() && !target.is_symlink() {
        info!("nothing to remove: {}", target.display());
        return Ok(());
    }

    if dry_run {
        info!("[dry-run] would remove {}", target.display());
    } else {
        std::fs::remove_file(target)
            .with_context(|| format!("removing {}", target.display()))?;
        info!("removed {}", target.display());
    }

    // Restore backup if one exists
    let backup = target.with_extension("aegis-backup");
    if backup.exists() && !dry_run {
        std::fs::rename(&backup, target)
            .with_context(|| format!("restoring backup {}", backup.display()))?;
        info!("restored backup {}", target.display());
    }

    Ok(())
}

/// Check if a deployed config matches the source.
pub fn check_config(source: &Path, target: &Path, strategy: LinkStrategy) -> ConfigStatus {
    match strategy {
        LinkStrategy::Symlink => {
            if !target.exists() && !target.is_symlink() {
                return ConfigStatus::Missing;
            }
            if target.is_symlink() {
                match std::fs::read_link(target) {
                    Ok(link) if link == source => ConfigStatus::Ok,
                    Ok(link) => ConfigStatus::Drifted(format!(
                        "symlink points to {} instead of {}",
                        link.display(),
                        source.display()
                    )),
                    Err(e) => ConfigStatus::Error(e.to_string()),
                }
            } else {
                ConfigStatus::Drifted("exists but is not a symlink".to_string())
            }
        }
        LinkStrategy::Copy | LinkStrategy::Template => {
            if !target.exists() {
                return ConfigStatus::Missing;
            }
            match (std::fs::read(source), std::fs::read(target)) {
                (Ok(src), Ok(tgt)) if src == tgt => ConfigStatus::Ok,
                (Ok(_), Ok(_)) => ConfigStatus::Drifted("file contents differ".to_string()),
                (Err(e), _) | (_, Err(e)) => ConfigStatus::Error(e.to_string()),
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigStatus {
    Ok,
    Missing,
    Drifted(String),
    Error(String),
}
