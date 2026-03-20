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
    /// Optional OS filter: "linux", "windows", "macos". Skipped if current OS doesn't match.
    #[serde(default)]
    pub os: Option<String>,
}

impl ConfigMapping {
    /// Returns true if this config applies to the current platform.
    pub fn applies_to_current_os(&self) -> bool {
        match self.os.as_deref() {
            None => true,
            Some("linux") => cfg!(target_os = "linux"),
            Some("windows") => cfg!(target_os = "windows"),
            Some("macos") => cfg!(target_os = "macos"),
            Some(_) => true,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn deploy_copy_creates_file() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        std::fs::write(&source, "hello").unwrap();

        deploy_config(&source, &target, LinkStrategy::Copy, false).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn deploy_symlink_creates_link() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        std::fs::write(&source, "hello").unwrap();

        deploy_config(&source, &target, LinkStrategy::Symlink, false).unwrap();
        assert!(target.is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn deploy_symlink_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        std::fs::write(&source, "hello").unwrap();

        deploy_config(&source, &target, LinkStrategy::Symlink, false).unwrap();
        // Second deploy should not error
        deploy_config(&source, &target, LinkStrategy::Symlink, false).unwrap();
        assert!(target.is_symlink());
    }

    #[test]
    fn undeploy_removes_file() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target.txt");
        std::fs::write(&target, "hello").unwrap();

        undeploy_config(&target, false).unwrap();
        assert!(!target.exists());
    }

    #[test]
    fn undeploy_nonexistent_is_ok() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("nonexistent.txt");
        undeploy_config(&target, false).unwrap();
    }

    #[test]
    fn check_config_ok_symlink() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        std::fs::write(&source, "hello").unwrap();

        deploy_config(&source, &target, LinkStrategy::Symlink, false).unwrap();
        assert_eq!(check_config(&source, &target, LinkStrategy::Symlink), ConfigStatus::Ok);
    }

    #[test]
    fn check_config_missing() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        std::fs::write(&source, "hello").unwrap();

        assert_eq!(check_config(&source, &target, LinkStrategy::Symlink), ConfigStatus::Missing);
    }

    #[test]
    fn check_config_ok_copy() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        std::fs::write(&source, "hello").unwrap();

        deploy_config(&source, &target, LinkStrategy::Copy, false).unwrap();
        assert_eq!(check_config(&source, &target, LinkStrategy::Copy), ConfigStatus::Ok);
    }

    #[test]
    fn check_config_drifted_copy() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        std::fs::write(&source, "hello").unwrap();
        std::fs::write(&target, "changed").unwrap();

        match check_config(&source, &target, LinkStrategy::Copy) {
            ConfigStatus::Drifted(_) => {}
            other => panic!("expected Drifted, got {other:?}"),
        }
    }

    #[test]
    fn dry_run_does_not_create_file() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        std::fs::write(&source, "hello").unwrap();

        deploy_config(&source, &target, LinkStrategy::Copy, true).unwrap();
        assert!(!target.exists());
    }

    #[test]
    fn deploy_source_missing_errors() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("nonexistent.txt");
        let target = dir.path().join("target.txt");

        assert!(deploy_config(&source, &target, LinkStrategy::Copy, false).is_err());
    }
}
