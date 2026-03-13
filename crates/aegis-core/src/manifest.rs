use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::module::ModuleRef;
use crate::profile::Profile;
use crate::variables::VariableSource;

/// Top-level aegis.toml manifest.
#[derive(Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub aegis: AegisConfig,
    #[serde(default)]
    pub variables: HashMap<String, VariableSource>,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
    #[serde(default)]
    pub modules: Vec<ModuleRef>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AegisConfig {
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_strategy")]
    pub strategy: LinkStrategy,
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default = "default_secrets_backend")]
    pub secrets_backend: SecretsBackend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkStrategy {
    Symlink,
    Copy,
    Template,
}

fn default_strategy() -> LinkStrategy {
    LinkStrategy::Symlink
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretsBackend {
    Env,
}

fn default_secrets_backend() -> SecretsBackend {
    SecretsBackend::Env
}

impl Manifest {
    /// Load a manifest from the given path.
    pub fn load(path: &Path) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let manifest: Manifest =
            toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
        Ok(manifest)
    }

    /// Find the manifest file by searching upward from the given directory.
    pub fn find(start: &Path) -> Option<PathBuf> {
        let mut current = start.to_path_buf();
        loop {
            let candidate = current.join("aegis.toml");
            if candidate.is_file() {
                return Some(candidate);
            }
            if !current.pop() {
                return None;
            }
        }
    }

    /// Resolve module paths relative to the manifest's parent directory.
    pub fn resolve_module_paths(&self, manifest_dir: &Path) -> Vec<PathBuf> {
        self.modules
            .iter()
            .map(|m| manifest_dir.join(&m.path))
            .collect()
    }

    /// Get the active profile, falling back to default_profile or the first defined profile.
    pub fn active_profile<'a>(&'a self, override_name: Option<&'a str>) -> Option<(&'a str, &'a Profile)> {
        if let Some(name) = override_name {
            return self.profiles.get(name).map(|p| (name, p));
        }
        if let Some(ref default) = self.aegis.default_profile {
            return self
                .profiles
                .get(default.as_str())
                .map(|p| (default.as_str(), p));
        }
        self.profiles.iter().next().map(|(k, v)| (k.as_str(), v))
    }
}

impl Default for AegisConfig {
    fn default() -> Self {
        Self {
            version: "0.1.0".to_string(),
            description: None,
            strategy: LinkStrategy::Symlink,
            default_profile: None,
            secrets_backend: SecretsBackend::Env,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manifest() {
        let toml_str = r#"
[aegis]
version = "0.1.0"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.aegis.version, "0.1.0");
        assert_eq!(manifest.aegis.strategy, LinkStrategy::Symlink);
        assert!(manifest.modules.is_empty());
    }

    #[test]
    fn parse_full_manifest() {
        let toml_str = r#"
[aegis]
version = "0.1.0"
description = "My system"
default_profile = "dev-vps"
strategy = "symlink"

[variables]
hostname = { source = "command", value = "hostname" }
user = { source = "env", value = "USER" }

[profiles.dev-vps]
description = "Development VPS"
modules = ["shell", "terminal"]

[profiles.ci]
description = "CI environment"
modules = ["shell"]

[[modules]]
name = "shell"
path = "modules/shell"

[[modules]]
name = "terminal"
path = "modules/terminal"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.aegis.default_profile.as_deref(), Some("dev-vps"));
        assert_eq!(manifest.modules.len(), 2);
        assert_eq!(manifest.profiles.len(), 2);
        assert_eq!(manifest.variables.len(), 2);
    }

    #[test]
    fn active_profile_override() {
        let toml_str = r#"
[aegis]
version = "0.1.0"
default_profile = "dev-vps"

[profiles.dev-vps]
description = "Development VPS"
modules = ["shell"]

[profiles.ci]
description = "CI"
modules = []
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();

        // Default profile
        let (name, _) = manifest.active_profile(None).unwrap();
        assert_eq!(name, "dev-vps");

        // Override
        let (name, profile) = manifest.active_profile(Some("ci")).unwrap();
        assert_eq!(name, "ci");
        assert_eq!(profile.description.as_deref(), Some("CI"));

        // Unknown override returns None
        assert!(manifest.active_profile(Some("nonexistent")).is_none());
    }

    #[test]
    fn default_strategy_is_symlink() {
        let toml_str = r#"
[aegis]
version = "0.1.0"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.aegis.strategy, LinkStrategy::Symlink);
    }

    #[test]
    fn copy_strategy() {
        let toml_str = r#"
[aegis]
version = "0.1.0"
strategy = "copy"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.aegis.strategy, LinkStrategy::Copy);
    }

    #[test]
    fn roundtrip_serialize() {
        let toml_str = r#"
[aegis]
version = "0.1.0"
strategy = "symlink"

[[modules]]
name = "shell"
path = "modules/shell"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        let serialized = toml::to_string_pretty(&manifest).unwrap();
        let reparsed: Manifest = toml::from_str(&serialized).unwrap();
        assert_eq!(manifest.aegis.version, reparsed.aegis.version);
        assert_eq!(manifest.modules.len(), reparsed.modules.len());
    }
}
