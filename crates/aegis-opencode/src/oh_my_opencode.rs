use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::agents::{AgentInput, AgentJson, CategoryInput, CategoryJson};
use crate::models::ModelInput;

/// oh-my-opencode section from aegis TOML.
#[derive(Debug, Deserialize)]
pub struct OhMyOpencodeInput {
    #[serde(default)]
    pub disabled_hooks: Vec<String>,
    #[serde(default)]
    pub agents: HashMap<String, AgentInput>,
    #[serde(default)]
    pub categories: HashMap<String, CategoryInput>,
}

/// The output oh-my-opencode.json structure.
#[derive(Debug, Serialize)]
pub struct OhMyOpencodeJson {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disabled_hooks: Vec<String>,
    pub agents: HashMap<String, AgentJson>,
    pub categories: HashMap<String, CategoryJson>,
}

impl OhMyOpencodeInput {
    /// Load from a TOML file containing an `[oh_my_opencode]` section.
    pub fn load(path: &Path) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

        #[derive(Deserialize)]
        struct Wrapper {
            oh_my_opencode: OhMyOpencodeInput,
        }

        let wrapper: Wrapper =
            toml::from_str(&content).with_context(|| format!("parsing {}", path.display()))?;
        Ok(wrapper.oh_my_opencode)
    }

    /// Generate the oh-my-opencode.json output.
    ///
    /// `models` is needed to resolve model keys to full `provider/model_id` format.
    pub fn generate(
        &self,
        models: &HashMap<String, ModelInput>,
    ) -> Result<OhMyOpencodeJson> {
        let agents = self.build_agents(models)?;
        let categories = self.build_categories(models)?;

        Ok(OhMyOpencodeJson {
            schema: Some(
                "https://raw.githubusercontent.com/code-yeongyu/oh-my-opencode/master/assets/oh-my-opencode.schema.json".to_string(),
            ),
            disabled_hooks: self.disabled_hooks.clone(),
            agents,
            categories,
        })
    }

    fn build_agents(
        &self,
        models: &HashMap<String, ModelInput>,
    ) -> Result<HashMap<String, AgentJson>> {
        let mut agents = HashMap::new();

        for (name, input) in &self.agents {
            let full_model = resolve_model_ref(&input.model, models)?;
            agents.insert(
                name.clone(),
                AgentJson {
                    model: full_model,
                    temperature: input.temperature,
                    top_p: input.top_p,
                    max_tokens: input.max_tokens,
                },
            );
        }

        Ok(agents)
    }

    fn build_categories(
        &self,
        models: &HashMap<String, ModelInput>,
    ) -> Result<HashMap<String, CategoryJson>> {
        let mut categories = HashMap::new();

        for (name, input) in &self.categories {
            let full_model = resolve_model_ref(&input.model, models)?;
            categories.insert(
                name.clone(),
                CategoryJson { model: full_model },
            );
        }

        Ok(categories)
    }
}

/// Resolve a model key to the full `provider/model_id` string.
fn resolve_model_ref(
    model_key: &str,
    models: &HashMap<String, ModelInput>,
) -> Result<String> {
    if let Some(model) = models.get(model_key) {
        Ok(format!("{}/{}", model.provider, model.model_id))
    } else if model_key.contains('/') {
        // Already in provider/model_id format
        Ok(model_key.to_string())
    } else {
        anyhow::bail!(
            "model '{}' not found in defined models (available: {})",
            model_key,
            models.keys().cloned().collect::<Vec<_>>().join(", ")
        );
    }
}

/// Serialize the generated config to pretty-printed JSON.
pub fn to_json_string(config: &OhMyOpencodeJson) -> Result<String> {
    serde_json::to_string_pretty(config).context("serializing oh-my-opencode.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_oh_my_opencode_json() {
        let toml_str = r#"
[opencode.models.qwen3-5-122b]
provider = "nvidia"
model_id = "qwen/qwen3.5-122b-a10b"

[opencode.models.glm4-7]
provider = "nvidia"
model_id = "z-ai/glm4.7"

[oh_my_opencode]
disabled_hooks = ["category-skill-reminder"]

[oh_my_opencode.agents.sisyphus]
model = "qwen3-5-122b"
temperature = 0.6
max_tokens = 32768

[oh_my_opencode.agents.librarian]
model = "glm4-7"

[oh_my_opencode.categories.deep]
model = "qwen3-5-122b"
"#;

        #[derive(Deserialize)]
        struct Wrapper {
            opencode: OpencodeModels,
            oh_my_opencode: OhMyOpencodeInput,
        }
        #[derive(Deserialize)]
        struct OpencodeModels {
            models: HashMap<String, ModelInput>,
        }

        let wrapper: Wrapper = toml::from_str(toml_str).unwrap();
        let json = wrapper
            .oh_my_opencode
            .generate(&wrapper.opencode.models)
            .unwrap();

        assert_eq!(
            json.agents.get("sisyphus").unwrap().model,
            "nvidia/qwen/qwen3.5-122b-a10b"
        );
        assert_eq!(
            json.agents.get("librarian").unwrap().model,
            "nvidia/z-ai/glm4.7"
        );
        assert_eq!(
            json.categories.get("deep").unwrap().model,
            "nvidia/qwen/qwen3.5-122b-a10b"
        );
    }

    #[test]
    fn unknown_agent_model_errors() {
        let toml_str = r#"
[opencode.models.qwen3-5-122b]
provider = "nvidia"
model_id = "qwen/qwen3.5-122b-a10b"

[oh_my_opencode]

[oh_my_opencode.agents.bad-agent]
model = "nonexistent-model"
"#;
        #[derive(Deserialize)]
        struct Wrapper {
            opencode: OpencodeModels,
            oh_my_opencode: OhMyOpencodeInput,
        }
        #[derive(Deserialize)]
        struct OpencodeModels {
            models: HashMap<String, ModelInput>,
        }

        let wrapper: Wrapper = toml::from_str(toml_str).unwrap();
        let result = wrapper.oh_my_opencode.generate(&wrapper.opencode.models);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn unknown_category_model_errors() {
        let toml_str = r#"
[opencode.models.qwen3-5-122b]
provider = "nvidia"
model_id = "qwen/qwen3.5-122b-a10b"

[oh_my_opencode]

[oh_my_opencode.categories.bad-cat]
model = "nonexistent-model"
"#;
        #[derive(Deserialize)]
        struct Wrapper {
            opencode: OpencodeModels,
            oh_my_opencode: OhMyOpencodeInput,
        }
        #[derive(Deserialize)]
        struct OpencodeModels {
            models: HashMap<String, ModelInput>,
        }

        let wrapper: Wrapper = toml::from_str(toml_str).unwrap();
        let result = wrapper.oh_my_opencode.generate(&wrapper.opencode.models);
        assert!(result.is_err());
    }

    #[test]
    fn full_model_ref_in_agent_passes() {
        let models = HashMap::new(); // No defined models
        let input = OhMyOpencodeInput {
            disabled_hooks: vec![],
            agents: {
                let mut m = HashMap::new();
                m.insert(
                    "test".to_string(),
                    AgentInput {
                        model: "nvidia/qwen/qwen3.5-122b".to_string(),
                        temperature: None,
                        top_p: None,
                        max_tokens: None,
                    },
                );
                m
            },
            categories: HashMap::new(),
        };

        let json = input.generate(&models).unwrap();
        assert_eq!(
            json.agents.get("test").unwrap().model,
            "nvidia/qwen/qwen3.5-122b"
        );
    }
}
