pub mod bootstrap;
pub mod diff;
pub mod enforce;
pub mod init;
pub mod inventory;
pub mod link;
pub mod net;
pub mod opencode;
pub mod profile;
pub mod push;
pub mod remote;
pub mod secrets;
pub mod status;
pub mod sync;
pub mod toolchain;
pub mod watch;

use std::path::{Path, PathBuf};

use aegis_core::manifest::Manifest;
use aegis_core::module::Module;
use anyhow::{Context as _, Result, bail};

/// Shared context for all commands.
pub struct Context {
    pub config_path: Option<String>,
    pub profile: Option<String>,
    pub dry_run: bool,
    #[allow(dead_code)]
    pub verbose: bool,
}

impl Context {
    /// Find and load the manifest.
    pub fn load_manifest(&self) -> Result<(Manifest, PathBuf)> {
        let manifest_path = if let Some(ref path) = self.config_path {
            PathBuf::from(path)
        } else {
            let cwd = std::env::current_dir().context("getting current directory")?;
            Manifest::find(&cwd)
                .ok_or_else(|| anyhow::anyhow!("no aegis.toml found (searched upward from {})", cwd.display()))?
        };

        let manifest = Manifest::load(&manifest_path)?;
        let manifest_dir = manifest_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        Ok((manifest, manifest_dir))
    }

    /// Load all modules, optionally filtered by a single module name.
    pub fn load_modules(
        &self,
        manifest: &Manifest,
        manifest_dir: &Path,
        filter: Option<&str>,
    ) -> Result<Vec<Module>> {
        let profile = manifest.active_profile(self.profile.as_deref());
        let active_modules: Option<Vec<&str>> = profile.map(|(_, p)| {
            p.modules.iter().map(|s| s.as_str()).collect()
        });

        let mut modules = Vec::new();
        for module_ref in &manifest.modules {
            // Filter by specific module name if provided
            if let Some(name) = filter {
                if module_ref.name != name {
                    continue;
                }
            }

            // Filter by profile if one is active
            if let Some(ref active) = active_modules {
                if !active.contains(&module_ref.name.as_str()) {
                    continue;
                }
            }

            let module_dir = manifest_dir.join(&module_ref.path);
            if !module_dir.exists() {
                if filter.is_some() {
                    bail!("module directory not found: {}", module_dir.display());
                }
                tracing::warn!("skipping missing module dir: {}", module_dir.display());
                continue;
            }

            let module = Module::load(&module_dir)
                .with_context(|| format!("loading module '{}'", module_ref.name))?;
            modules.push(module);
        }

        Ok(modules)
    }
}
