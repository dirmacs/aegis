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
