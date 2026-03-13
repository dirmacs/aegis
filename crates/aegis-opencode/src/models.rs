use std::collections::HashMap;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// Provider definition in aegis TOML.
#[derive(Debug, Deserialize)]
pub struct ProviderInput {
    /// NPM package for the provider SDK.
    #[serde(default = "default_npm")]
    pub npm: String,
    /// Display name for the provider.
    #[serde(default)]
    pub name: Option<String>,
    /// Provider type (e.g., "openai-compatible").
    #[serde(rename = "type", default)]
    pub provider_type: Option<String>,
    /// Environment variable name holding the API key.
    #[serde(default)]
    pub api_key_env: Option<String>,
    /// Base URL for the API.
    #[serde(default)]
    pub base_url: Option<String>,
}

fn default_npm() -> String {
    "@ai-sdk/openai-compatible".to_string()
}

/// Model definition in aegis TOML.
#[derive(Debug, Deserialize)]
pub struct ModelInput {
    /// Which provider this model belongs to.
    pub provider: String,
    /// The model identifier for the API.
    pub model_id: String,
    /// Display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Context window length.
    #[serde(default)]
    pub context_length: Option<u64>,
    /// Max output tokens.
    #[serde(default)]
    pub max_output: Option<u64>,
    /// Temperature.
    #[serde(default)]
    pub temperature: Option<f64>,
    /// Top P.
    #[serde(default)]
    pub top_p: Option<f64>,
    /// Top K.
    #[serde(default)]
    pub top_k: Option<u32>,
    /// Enable thinking/chain-of-thought.
    #[serde(default)]
    pub thinking: Option<bool>,
    /// Disable clear thinking.
    #[serde(default)]
    pub clear_thinking_disabled: Option<bool>,
}

impl ModelInput {
    /// Validate numeric parameter ranges.
    pub fn validate(&self, key: &str) -> Result<()> {
        if let Some(ctx) = self.context_length {
            if ctx == 0 {
                bail!("model '{key}': context_length must be > 0");
            }
        }
        if let Some(out) = self.max_output {
            if out == 0 {
                bail!("model '{key}': max_output must be > 0");
            }
        }
        if let Some(temp) = self.temperature {
            if !(0.0..=2.0).contains(&temp) {
                bail!("model '{key}': temperature must be in [0.0, 2.0], got {temp}");
            }
        }
        if let Some(p) = self.top_p {
            if !(0.0..=1.0).contains(&p) {
                bail!("model '{key}': top_p must be in [0.0, 1.0], got {p}");
            }
        }
        Ok(())
    }
}

// --- Output types that serialize to opencode.json format ---

/// Provider object in opencode.json.
#[derive(Debug, Serialize)]
pub struct ProviderJson {
    pub npm: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<ProviderOptionsJson>,
    pub models: HashMap<String, ModelJson>,
}

#[derive(Debug, Serialize)]
pub struct ProviderOptionsJson {
    #[serde(rename = "baseURL", skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(rename = "apiKey", skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Model object nested inside a provider in opencode.json.
#[derive(Debug, Serialize)]
pub struct ModelJson {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<ModelLimitJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<ModelParametersJson>,
}

#[derive(Debug, Serialize)]
pub struct ModelLimitJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ModelParametersJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_kwargs: Option<ChatTemplateKwargs>,
}

#[derive(Debug, Serialize)]
pub struct ChatTemplateKwargs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_clear_thinking: Option<bool>,
}
