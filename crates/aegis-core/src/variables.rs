use std::collections::HashMap;
use std::process::Command;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Where a variable's value comes from.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "source", content = "value")]
pub enum VariableSource {
    /// Read from an environment variable.
    #[serde(rename = "env")]
    Env(String),
    /// Run a shell command and capture stdout.
    #[serde(rename = "command")]
    Command(String),
    /// A static literal value.
    #[serde(rename = "static")]
    Static(String),
}

/// Resolve all variables to their string values.
pub fn resolve_variables(
    sources: &HashMap<String, VariableSource>,
    profile_overrides: &HashMap<String, String>,
) -> Result<HashMap<String, String>> {
    let mut resolved = HashMap::new();

    for (key, source) in sources {
        // Profile overrides take precedence
        if let Some(override_val) = profile_overrides.get(key) {
            resolved.insert(key.clone(), override_val.clone());
            continue;
        }

        match resolve_single(source) {
            Ok(val) => {
                resolved.insert(key.clone(), val);
            }
            Err(e) => {
                warn!("failed to resolve variable '{}': {}", key, e);
                // Still insert as empty so templates don't error on missing vars
                resolved.insert(key.clone(), String::new());
            }
        }
    }

    // Also add profile overrides that don't have a source definition
    for (key, val) in profile_overrides {
        if !resolved.contains_key(key) {
            resolved.insert(key.clone(), val.clone());
        }
    }

    Ok(resolved)
}

fn resolve_single(source: &VariableSource) -> Result<String> {
    match source {
        VariableSource::Env(var_name) => {
            std::env::var(var_name).map_err(|_| anyhow::anyhow!("env var {} not set", var_name))
        }
        VariableSource::Command(cmd) => {
            let output = Command::new("sh")
                .args(["-c", cmd])
                .output()?;
            if !output.status.success() {
                bail!("command '{}' failed", cmd);
            }
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        VariableSource::Static(val) => Ok(val.clone()),
    }
}

/// Check which required env vars are set.
pub fn check_env_vars(sources: &HashMap<String, VariableSource>) -> Vec<EnvVarStatus> {
    sources
        .iter()
        .filter_map(|(key, source)| {
            if let VariableSource::Env(var_name) = source {
                let set = std::env::var(var_name).is_ok();
                Some(EnvVarStatus {
                    variable_key: key.clone(),
                    env_var: var_name.clone(),
                    set,
                })
            } else {
                None
            }
        })
        .collect()
}

#[derive(Debug)]
pub struct EnvVarStatus {
    pub variable_key: String,
    pub env_var: String,
    pub set: bool,
}
