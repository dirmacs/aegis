use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageSpec {
    pub name: String,
    pub install_method: InstallMethod,
    #[serde(default)]
    pub cargo_crate: Option<String>,
    #[serde(default)]
    pub git_repo: Option<String>,
    #[serde(default)]
    pub version_check: Option<String>,
    #[serde(default)]
    pub expected_version: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallMethod {
    Cargo,
    Apt,
    Script,
    Mise,
}

#[derive(Debug)]
pub struct PackageStatus {
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
    pub version_ok: bool,
}

impl PackageSpec {
    /// Check if this package is installed and get its version.
    pub fn check_status(&self) -> PackageStatus {
        let installed = self.is_installed();
        let version = if installed {
            self.get_version()
        } else {
            None
        };
        let version_ok = match (&version, &self.expected_version) {
            (Some(v), Some(expected)) => v.contains(expected),
            (Some(_), None) => true,
            _ => false,
        };

        PackageStatus {
            name: self.name.clone(),
            installed,
            version,
            version_ok,
        }
    }

    /// Check if the package binary is available.
    fn is_installed(&self) -> bool {
        // Try `which` on the package name first
        which::which(&self.name).is_ok()
    }

    /// Get the installed version string.
    fn get_version(&self) -> Option<String> {
        let check = self.version_check.as_ref()?;
        let parts: Vec<&str> = check.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        let output = Command::new(parts[0]).args(&parts[1..]).output().ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Version info might be on stdout or stderr
        let text = if stdout.trim().is_empty() {
            stderr.to_string()
        } else {
            stdout.to_string()
        };
        Some(text.trim().to_string())
    }

    /// Install this package.
    pub fn install(&self, dry_run: bool) -> Result<()> {
        match self.install_method {
            InstallMethod::Cargo => self.install_cargo(dry_run),
            InstallMethod::Apt => self.install_apt(dry_run),
            InstallMethod::Script => bail!("script-based install not yet implemented for {}", self.name),
            InstallMethod::Mise => bail!("mise-based install not yet implemented for {}", self.name),
        }
    }

    fn install_cargo(&self, dry_run: bool) -> Result<()> {
        let crate_name = self.cargo_crate.as_deref().unwrap_or(&self.name);
        let mut args = vec!["install", crate_name];

        let features_str: String;
        if !self.features.is_empty() {
            features_str = self.features.join(",");
            args.push("--features");
            args.push(&features_str);
        }

        if dry_run {
            info!("[dry-run] would run: cargo {}", args.join(" "));
            return Ok(());
        }

        info!("installing {} via cargo", crate_name);
        let status = Command::new("cargo")
            .args(&args)
            .status()
            .with_context(|| format!("running cargo install {crate_name}"))?;

        if !status.success() {
            bail!("cargo install {crate_name} failed with exit code {:?}", status.code());
        }
        Ok(())
    }

    fn install_apt(&self, dry_run: bool) -> Result<()> {
        if dry_run {
            info!("[dry-run] would run: sudo apt-get install -y {}", self.name);
            return Ok(());
        }

        info!("installing {} via apt", self.name);
        let status = Command::new("sudo")
            .args(["apt-get", "install", "-y", &self.name])
            .status()
            .with_context(|| format!("running apt-get install {}", self.name))?;

        if !status.success() {
            bail!("apt-get install {} failed", self.name);
        }
        Ok(())
    }
}
