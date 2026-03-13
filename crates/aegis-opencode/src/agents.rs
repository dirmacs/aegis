use serde::{Deserialize, Serialize};

/// Agent definition in aegis TOML (for oh-my-opencode).
#[derive(Debug, Deserialize)]
pub struct AgentInput {
    /// Model key referencing a model in the opencode models table.
    pub model: String,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
}

/// Category definition in aegis TOML (for oh-my-opencode).
#[derive(Debug, Deserialize)]
pub struct CategoryInput {
    /// Model key referencing a model in the opencode models table.
    pub model: String,
}

// --- Output types for oh-my-opencode.json ---

/// Agent object in oh-my-opencode.json.
#[derive(Debug, Serialize)]
pub struct AgentJson {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
}

/// Category object in oh-my-opencode.json.
#[derive(Debug, Serialize)]
pub struct CategoryJson {
    pub model: String,
}
