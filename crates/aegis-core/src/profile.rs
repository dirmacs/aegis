use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A profile defines which modules to enable and variable overrides.
#[derive(Debug, Deserialize, Serialize)]
pub struct Profile {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

impl Profile {
    /// Check if a module is enabled in this profile.
    pub fn has_module(&self, name: &str) -> bool {
        self.modules.iter().any(|m| m == name)
    }
}
