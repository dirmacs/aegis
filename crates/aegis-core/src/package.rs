use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Deserialize, Serialize)]
pub struct PackageSpec {
    pub name: String,
    /// Install method. Defaults to "auto" which resolves at runtime per platform.
    #[serde(default)]
    pub install_method: InstallMethod,
    /// Human description — used by LLM resolver when method is auto.
    #[serde(default)]
    pub description: Option<String>,
    /// Binary name to check on PATH (defaults to package name).
    #[serde(default)]
    pub binary: Option<String>,
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
    #[serde(default)]
    pub scoop_package: Option<String>,
    #[serde(default)]
    pub winget_id: Option<String>,
    #[serde(default)]
    pub scoop_bucket: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallMethod {
    /// Resolve at runtime — heuristic → LLM → fallback
    Auto,
    Cargo,
    Apt,
    Scoop,
    Winget,
    Script,
    Mise,
}

impl Default for InstallMethod {
    fn default() -> Self {
        Self::Auto
    }
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
        let binary = self.binary.as_deref().unwrap_or(&self.name);
        which::which(binary).is_ok()
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
            InstallMethod::Auto => self.install_auto(dry_run),
            InstallMethod::Cargo => self.install_cargo(dry_run),
            InstallMethod::Apt => self.install_apt(dry_run),
            InstallMethod::Scoop => self.install_scoop(dry_run),
            InstallMethod::Winget => self.install_winget(dry_run),
            InstallMethod::Script => bail!("script-based install not yet implemented for {}", self.name),
            InstallMethod::Mise => bail!("mise-based install not yet implemented for {}", self.name),
        }
    }

    fn install_auto(&self, dry_run: bool) -> Result<()> {
        use crate::resolver::{self, ResolveCache};

        let mut cache = ResolveCache::load();
        let desc = self.description.as_deref().unwrap_or(&self.name);

        let resolved = resolver::resolve(&self.name, desc, &mut cache)
            .ok_or_else(|| anyhow::anyhow!(
                "could not resolve install method for '{}' on {}/{}",
                self.name, std::env::consts::OS, std::env::consts::ARCH,
            ))?;

        let _ = cache.save();

        info!(
            "resolved '{}' → {:?} ({}), source: {:?}",
            self.name, resolved.method, resolved.manager_package, resolved.source
        );

        // Try the resolved method
        let result = self.execute_resolved(&resolved, dry_run);

        if let Err(ref e) = result {
            if dry_run {
                return result;
            }
            // Agentic retry: feed the error back to the LLM for an alternative
            let error_msg = format!("{e}");
            info!("first attempt failed for '{}', asking agent for alternative...", self.name);

            if let Some(retry) = resolver::resolve_with_retry(
                &self.name, desc, &mut cache, &error_msg,
            ) {
                info!(
                    "retry resolved '{}' → {:?} ({})",
                    self.name, retry.method, retry.manager_package
                );
                let _ = cache.save();
                return self.execute_resolved(&retry, dry_run);
            }
        }

        result
    }

    /// Execute a resolved install instruction.
    fn execute_resolved(
        &self,
        resolved: &crate::resolver::ResolvedInstall,
        dry_run: bool,
    ) -> Result<()> {
        // Script-based installs (from LLM agent)
        if let Some(ref script) = resolved.script {
            if dry_run {
                info!("[dry-run] would run: {script}");
                return Ok(());
            }
            info!("installing {} via agent script: {script}", self.name);
            let status = Command::new("sh")
                .args(["-c", script])
                .status()
                .with_context(|| format!("running script for {}", self.name))?;
            if !status.success() {
                bail!("script install for {} failed", self.name);
            }
            return Ok(());
        }

        // Manager-based installs
        match resolved.method {
            InstallMethod::Cargo => {
                let spec = PackageSpec {
                    name: self.name.clone(),
                    install_method: InstallMethod::Cargo,
                    cargo_crate: Some(resolved.manager_package.clone()),
                    features: self.features.clone(),
                    ..default_spec()
                };
                spec.install_cargo(dry_run)
            }
            InstallMethod::Apt => {
                let spec = PackageSpec {
                    name: resolved.manager_package.clone(),
                    install_method: InstallMethod::Apt,
                    ..default_spec()
                };
                spec.install_apt(dry_run)
            }
            InstallMethod::Scoop => {
                let spec = PackageSpec {
                    name: self.name.clone(),
                    install_method: InstallMethod::Scoop,
                    scoop_package: Some(resolved.manager_package.clone()),
                    scoop_bucket: self.scoop_bucket.clone(),
                    ..default_spec()
                };
                spec.install_scoop(dry_run)
            }
            InstallMethod::Winget => {
                let spec = PackageSpec {
                    name: self.name.clone(),
                    install_method: InstallMethod::Winget,
                    winget_id: Some(resolved.manager_package.clone()),
                    ..default_spec()
                };
                spec.install_winget(dry_run)
            }
            InstallMethod::Script => {
                bail!("script method but no script provided for {}", self.name)
            }
            _ => bail!("resolved method {:?} not supported for auto-install", resolved.method),
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

    fn install_scoop(&self, dry_run: bool) -> Result<()> {
        let pkg = self.scoop_package.as_deref().unwrap_or(&self.name);

        // Add bucket if specified and not already added
        if let Some(bucket) = &self.scoop_bucket {
            if !dry_run {
                let _ = Command::new("scoop")
                    .args(["bucket", "add", bucket])
                    .status();
            }
        }

        if dry_run {
            info!("[dry-run] would run: scoop install {pkg}");
            return Ok(());
        }

        info!("installing {} via scoop", pkg);
        let status = Command::new("scoop")
            .args(["install", pkg])
            .status()
            .with_context(|| format!("running scoop install {pkg}"))?;

        if !status.success() {
            bail!("scoop install {pkg} failed");
        }
        Ok(())
    }

    fn install_winget(&self, dry_run: bool) -> Result<()> {
        let id = self.winget_id.as_deref().unwrap_or(&self.name);

        if dry_run {
            info!("[dry-run] would run: winget install {id}");
            return Ok(());
        }

        info!("installing {} via winget", id);
        let status = Command::new("winget")
            .args([
                "install", id,
                "--accept-package-agreements",
                "--accept-source-agreements",
            ])
            .status()
            .with_context(|| format!("running winget install {id}"))?;

        if !status.success() {
            bail!("winget install {id} failed");
        }
        Ok(())
    }
}

fn default_spec() -> PackageSpec {
    PackageSpec {
        name: String::new(),
        install_method: InstallMethod::Auto,
        description: None,
        binary: None,
        cargo_crate: None,
        git_repo: None,
        version_check: None,
        expected_version: None,
        features: Vec::new(),
        scoop_package: None,
        winget_id: None,
        scoop_bucket: None,
    }
}
