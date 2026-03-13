use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::mcp::{McpInput, McpJson};
use crate::models::{
    ChatTemplateKwargs, ModelInput, ModelJson, ModelLimitJson, ModelParametersJson, ProviderInput,
    ProviderJson, ProviderOptionsJson,
};

/// Top-level opencode section from aegis TOML.
#[derive(Debug, Deserialize)]
pub struct OpencodeInput {
    #[serde(default)]
    pub data_directory: Option<String>,
    #[serde(default)]
    pub auto_compact: Option<bool>,
    #[serde(default)]
    pub debug: Option<bool>,
    #[serde(default)]
    pub shell: Option<ShellInput>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderInput>,
    #[serde(default)]
    pub models: HashMap<String, ModelInput>,
    #[serde(default)]
    pub default_model: Option<DefaultModelInput>,
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpInput>,
    #[serde(default)]
    pub plugin: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShellInput {
    pub path: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct DefaultModelInput {
    pub model: String,
}

/// The output opencode.json structure.
#[derive(Debug, Serialize)]
pub struct OpencodeJson {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<HashMap<String, ProviderJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<HashMap<String, McpJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<Vec<String>>,
}

impl OpencodeInput {
    /// Load from a TOML file containing an `[opencode]` section.
    pub fn load(path: &Path) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

        // The file has [opencode.xxx] sections, so we need to parse the top-level
        #[derive(Deserialize)]
        struct Wrapper {
            opencode: OpencodeInput,
        }

        let wrapper: Wrapper =
            toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
        Ok(wrapper.opencode)
    }

    /// Generate the opencode.json output.
    pub fn generate(&self) -> Result<OpencodeJson> {
        // Validate all model parameters
        for (key, model) in &self.models {
            model.validate(key)?;
        }

        let providers = self.build_providers()?;
        let default_model = self.resolve_default_model()?;
        let mcp = self.build_mcp();
        let plugin = if self.plugin.is_empty() {
            None
        } else {
            Some(self.plugin.clone())
        };

        Ok(OpencodeJson {
            schema: Some("https://opencode.ai/config.json".to_string()),
            provider: Some(providers),
            model: Some(default_model),
            mcp: Some(mcp),
            plugin,
        })
    }

    /// Build the provider map, grouping models under their respective providers.
    fn build_providers(&self) -> Result<HashMap<String, ProviderJson>> {
        let mut providers: HashMap<String, ProviderJson> = HashMap::new();

        // Initialize providers from provider definitions
        for (provider_key, provider_input) in &self.providers {
            let options = if provider_input.base_url.is_some() || provider_input.api_key_env.is_some()
            {
                Some(ProviderOptionsJson {
                    base_url: provider_input.base_url.clone(),
                    api_key: provider_input
                        .api_key_env
                        .as_ref()
                        .map(|env| format!("{{env:{env}}}")),
                })
            } else {
                None
            };

            providers.insert(
                provider_key.clone(),
                ProviderJson {
                    npm: provider_input.npm.clone(),
                    name: provider_input
                        .name
                        .clone()
                        .unwrap_or_else(|| provider_key.clone()),
                    options,
                    models: HashMap::new(),
                },
            );
        }

        // Add models to their providers
        for model_input in self.models.values() {
            let provider = providers.get_mut(&model_input.provider).ok_or_else(|| {
                anyhow::anyhow!(
                    "model references unknown provider '{}'",
                    model_input.provider
                )
            })?;

            let limit = if model_input.context_length.is_some() || model_input.max_output.is_some()
            {
                Some(ModelLimitJson {
                    context: model_input.context_length,
                    output: model_input.max_output,
                })
            } else {
                None
            };

            let chat_template_kwargs =
                if model_input.thinking.is_some() || model_input.clear_thinking_disabled.is_some() {
                    Some(ChatTemplateKwargs {
                        enable_thinking: model_input.thinking,
                        disable_clear_thinking: model_input.clear_thinking_disabled,
                    })
                } else {
                    None
                };

            let parameters = if model_input.temperature.is_some()
                || model_input.top_p.is_some()
                || model_input.top_k.is_some()
                || chat_template_kwargs.is_some()
            {
                Some(ModelParametersJson {
                    temperature: model_input.temperature,
                    top_p: model_input.top_p,
                    top_k: model_input.top_k,
                    chat_template_kwargs,
                })
            } else {
                None
            };

            let name = model_input
                .name
                .clone()
                .unwrap_or_else(|| model_input.model_id.clone());

            provider.models.insert(
                model_input.model_id.clone(),
                ModelJson {
                    name,
                    limit,
                    parameters,
                },
            );
        }

        Ok(providers)
    }

    /// Resolve the default model to the full `provider/model_id` format.
    fn resolve_default_model(&self) -> Result<String> {
        let default = self
            .default_model
            .as_ref()
            .map(|d| d.model.as_str())
            .unwrap_or_else(|| {
                self.models.keys().next().map(|k| k.as_str()).unwrap_or("")
            });

        if default.is_empty() {
            bail!("no default model specified and no models defined");
        }

        // Look up the model key to get provider/model_id
        if let Some(model) = self.models.get(default) {
            Ok(format!("{}/{}", model.provider, model.model_id))
        } else if default.contains('/') {
            // Already in provider/model_id format
            Ok(default.to_string())
        } else {
            bail!(
                "default model '{}' not found in defined models (available: {})",
                default,
                self.models.keys().cloned().collect::<Vec<_>>().join(", ")
            );
        }
    }

    /// Build the MCP server map.
    fn build_mcp(&self) -> HashMap<String, McpJson> {
        self.mcp_servers
            .iter()
            .map(|(key, input)| (key.clone(), input.to_json()))
            .collect()
    }
}

/// Serialize the generated config to pretty-printed JSON.
pub fn to_json_string(config: &OpencodeJson) -> Result<String> {
    serde_json::to_string_pretty(config).context("serializing opencode.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_basic_opencode_json() {
        let toml_str = r#"
[opencode]
plugin = ["oh-my-opencode@latest"]

[opencode.providers.nvidia]
npm = "@ai-sdk/openai-compatible"
name = "NVIDIA NIM"
base_url = "https://integrate.api.nvidia.com/v1"

[opencode.models.qwen3-5-122b]
provider = "nvidia"
model_id = "qwen/qwen3.5-122b-a10b"
name = "Qwen 3.5 122B"
context_length = 262144
max_output = 16384
temperature = 0.6
thinking = true

[opencode.default_model]
model = "qwen3-5-122b"

[opencode.mcp_servers.daedra]
type = "stdio"
command = "daedra"
args = ["serve", "--transport", "stdio", "--quiet"]
"#;

        #[derive(Deserialize)]
        struct Wrapper {
            opencode: OpencodeInput,
        }
        let wrapper: Wrapper = toml::from_str(toml_str).unwrap();
        let json = wrapper.opencode.generate().unwrap();

        assert_eq!(json.model.as_deref(), Some("nvidia/qwen/qwen3.5-122b-a10b"));
        assert!(json.provider.as_ref().unwrap().contains_key("nvidia"));
        assert!(json.mcp.as_ref().unwrap().contains_key("daedra"));
        assert_eq!(json.plugin.as_ref().unwrap(), &["oh-my-opencode@latest"]);
    }

    #[test]
    fn unknown_default_model_errors() {
        let toml_str = r#"
[opencode]

[opencode.providers.nvidia]
npm = "@ai-sdk/openai-compatible"

[opencode.models.qwen3-5-122b]
provider = "nvidia"
model_id = "qwen/qwen3.5-122b-a10b"

[opencode.default_model]
model = "nonexistent-model"
"#;
        #[derive(Deserialize)]
        struct Wrapper {
            opencode: OpencodeInput,
        }
        let wrapper: Wrapper = toml::from_str(toml_str).unwrap();
        let result = wrapper.opencode.generate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn invalid_temperature_errors() {
        let toml_str = r#"
[opencode]

[opencode.providers.nvidia]
npm = "@ai-sdk/openai-compatible"

[opencode.models.bad]
provider = "nvidia"
model_id = "test/model"
temperature = 5.0
"#;
        #[derive(Deserialize)]
        struct Wrapper {
            opencode: OpencodeInput,
        }
        let wrapper: Wrapper = toml::from_str(toml_str).unwrap();
        let result = wrapper.opencode.generate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("temperature"));
    }

    #[test]
    fn invalid_top_p_errors() {
        let toml_str = r#"
[opencode]

[opencode.providers.nvidia]
npm = "@ai-sdk/openai-compatible"

[opencode.models.bad]
provider = "nvidia"
model_id = "test/model"
top_p = 1.5
"#;
        #[derive(Deserialize)]
        struct Wrapper {
            opencode: OpencodeInput,
        }
        let wrapper: Wrapper = toml::from_str(toml_str).unwrap();
        let result = wrapper.opencode.generate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("top_p"));
    }

    #[test]
    fn full_model_ref_as_default_passes() {
        let toml_str = r#"
[opencode]

[opencode.providers.nvidia]
npm = "@ai-sdk/openai-compatible"

[opencode.models.qwen3]
provider = "nvidia"
model_id = "qwen/qwen3.5-122b"

[opencode.default_model]
model = "nvidia/qwen/qwen3.5-122b"
"#;
        #[derive(Deserialize)]
        struct Wrapper {
            opencode: OpencodeInput,
        }
        let wrapper: Wrapper = toml::from_str(toml_str).unwrap();
        let json = wrapper.opencode.generate().unwrap();
        assert_eq!(json.model.as_deref(), Some("nvidia/qwen/qwen3.5-122b"));
    }
}
