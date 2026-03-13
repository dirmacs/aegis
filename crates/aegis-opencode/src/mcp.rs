use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// MCP server definition in aegis TOML.
#[derive(Debug, Deserialize)]
pub struct McpInput {
    /// "stdio" for local, "remote" for remote.
    #[serde(rename = "type")]
    pub mcp_type: McpType,
    /// Command parts for local MCP (first element is binary, rest are args).
    #[serde(default)]
    pub command: Option<String>,
    /// Arguments for the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// URL for remote MCP.
    #[serde(default)]
    pub url: Option<String>,
    /// Environment variables for local MCP.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Whether this MCP server is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Timeout in seconds.
    #[serde(default)]
    pub timeout: Option<u32>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum McpType {
    Stdio,
    Remote,
    Local,
}

// --- Output types for opencode.json ---

/// MCP server in opencode.json — discriminated union on `type`.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum McpJson {
    Local(McpLocalJson),
    Remote(McpRemoteJson),
}

#[derive(Debug, Serialize)]
pub struct McpLocalJson {
    #[serde(rename = "type")]
    pub mcp_type: String,
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<HashMap<String, String>>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct McpRemoteJson {
    #[serde(rename = "type")]
    pub mcp_type: String,
    pub url: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

impl McpInput {
    /// Convert to opencode.json format.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_json(&self) -> McpJson {
        match self.mcp_type {
            McpType::Remote => McpJson::Remote(McpRemoteJson {
                mcp_type: "remote".to_string(),
                url: self.url.clone().unwrap_or_default(),
                enabled: self.enabled,
                timeout: self.timeout,
            }),
            McpType::Stdio | McpType::Local => {
                let mut command = Vec::new();
                if let Some(ref cmd) = self.command {
                    command.push(cmd.clone());
                }
                command.extend(self.args.iter().cloned());

                let environment = if self.env.is_empty() {
                    None
                } else {
                    Some(self.env.clone())
                };

                McpJson::Local(McpLocalJson {
                    mcp_type: "local".to_string(),
                    command,
                    environment,
                    enabled: self.enabled,
                    timeout: self.timeout,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_mcp_to_json() {
        let input = McpInput {
            mcp_type: McpType::Stdio,
            command: Some("daedra".to_string()),
            args: vec!["serve".to_string(), "--quiet".to_string()],
            url: None,
            env: HashMap::new(),
            enabled: true,
            timeout: None,
        };

        match input.to_json() {
            McpJson::Local(local) => {
                assert_eq!(local.mcp_type, "local");
                assert_eq!(local.command, vec!["daedra", "serve", "--quiet"]);
                assert!(local.environment.is_none());
                assert!(local.enabled);
            }
            McpJson::Remote(_) => panic!("expected Local"),
        }
    }

    #[test]
    fn remote_mcp_to_json() {
        let input = McpInput {
            mcp_type: McpType::Remote,
            command: None,
            args: vec![],
            url: Some("https://mcp.context7.com/mcp".to_string()),
            env: HashMap::new(),
            enabled: true,
            timeout: Some(30),
        };

        match input.to_json() {
            McpJson::Remote(remote) => {
                assert_eq!(remote.mcp_type, "remote");
                assert_eq!(remote.url, "https://mcp.context7.com/mcp");
                assert_eq!(remote.timeout, Some(30));
            }
            McpJson::Local(_) => panic!("expected Remote"),
        }
    }

    #[test]
    fn stdio_mcp_with_env() {
        let mut env = HashMap::new();
        env.insert("API_KEY".to_string(), "$API_KEY".to_string());

        let input = McpInput {
            mcp_type: McpType::Stdio,
            command: Some("bunx".to_string()),
            args: vec!["tavily-mcp".to_string()],
            url: None,
            env,
            enabled: true,
            timeout: None,
        };

        match input.to_json() {
            McpJson::Local(local) => {
                let env = local.environment.unwrap();
                assert_eq!(env.get("API_KEY").unwrap(), "$API_KEY");
            }
            McpJson::Remote(_) => panic!("expected Local"),
        }
    }
}
