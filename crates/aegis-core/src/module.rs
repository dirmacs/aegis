use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::ConfigMapping;
use crate::manifest::LinkStrategy;
use crate::package::PackageSpec;

/// Reference to a module in the top-level manifest.
#[derive(Debug, Deserialize, Serialize)]
pub struct ModuleRef {
    pub name: String,
    pub path: String,
}

/// A module's own manifest (module.toml).
#[derive(Debug, Deserialize, Serialize)]
pub struct ModuleManifest {
    pub module: ModuleInfo,
    #[serde(default)]
    pub packages: Vec<PackageSpec>,
    #[serde(default)]
    pub configs: Vec<ConfigMapping>,
    #[serde(default)]
    pub hooks: Vec<Hook>,
    #[serde(default)]
    pub sync_rules: Vec<SyncRule>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ModuleInfo {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Hook {
    pub event: HookEvent,
    pub command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookEvent {
    PreLink,
    PostLink,
    PreUnlink,
    PostUnlink,
    PreBootstrap,
    PostBootstrap,
    PreSync,
    PostSync,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SyncRule {
    pub live_path: String,
    pub managed_path: String,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
}

/// A fully resolved module with its manifest and base path.
#[derive(Debug)]
pub struct Module {
    pub name: String,
    pub base_path: PathBuf,
    pub manifest: ModuleManifest,
}

impl Module {
    /// Load a module from the given directory.
    pub fn load(module_dir: &Path) -> Result<Self> {
        let manifest_path = module_dir.join("module.toml");
        let content = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let manifest: ModuleManifest = toml::from_str(&content)
            .with_context(|| format!("parsing {}", manifest_path.display()))?;
        Ok(Self {
            name: manifest.module.name.clone(),
            base_path: module_dir.to_path_buf(),
            manifest,
        })
    }

    /// Get the source path for a config file relative to this module.
    pub fn config_source_path(&self, config: &ConfigMapping) -> PathBuf {
        self.base_path.join(&config.source)
    }

    /// Get the resolved target path for a config file.
    pub fn config_target_path(&self, config: &ConfigMapping) -> Result<PathBuf> {
        let expanded = shellexpand::tilde(&config.target);
        Ok(PathBuf::from(expanded.as_ref()))
    }

    /// Get the effective strategy for a config, falling back to the given default.
    pub fn effective_strategy(
        &self,
        config: &ConfigMapping,
        default: LinkStrategy,
    ) -> LinkStrategy {
        config.strategy.unwrap_or(default)
    }

    /// Filter hooks by event type.
    pub fn hooks_for(&self, event: HookEvent) -> Vec<&Hook> {
        self.manifest
            .hooks
            .iter()
            .filter(|h| h.event == event)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_module_manifest() {
        let toml_str = r#"
[module]
name = "shell"
description = "Shell configuration"

[[packages]]
name = "starship"
install_method = "cargo"
cargo_crate = "starship"
version_check = "starship --version"

[[configs]]
source = "bashrc"
target = "~/.bashrc"
strategy = "symlink"

[[hooks]]
event = "post-link"
command = "echo done"

[[sync_rules]]
live_path = "~/.bashrc"
managed_path = "bashrc"
"#;
        let manifest: ModuleManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.module.name, "shell");
        assert_eq!(manifest.packages.len(), 1);
        assert_eq!(manifest.configs.len(), 1);
        assert_eq!(manifest.hooks.len(), 1);
        assert_eq!(manifest.sync_rules.len(), 1);
    }
}
