use serde::{Deserialize, Serialize};

/// A tool in the dirmacs ecosystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    pub name: String,
    pub description: String,
    pub cargo_crate: String,
    pub git_repo: String,
    pub binary_name: Option<String>,
    pub version_check: Option<String>,
}

/// The built-in registry of dirmacs tools.
pub fn dirmacs_registry() -> Vec<ToolEntry> {
    vec![
        ToolEntry {
            name: "ares".to_string(),
            description: "Agentic retrieval-enhanced server — multi-provider LLM orchestration"
                .to_string(),
            cargo_crate: "ares-server".to_string(),
            git_repo: "https://github.com/dirmacs/ares".to_string(),
            binary_name: Some("ares-server".to_string()),
            version_check: Some("ares-server --version".to_string()),
        },
        ToolEntry {
            name: "daedra".to_string(),
            description: "Web search MCP server powered by DuckDuckGo".to_string(),
            cargo_crate: "daedra".to_string(),
            git_repo: "https://github.com/dirmacs/daedra".to_string(),
            binary_name: Some("daedra".to_string()),
            version_check: Some("daedra --version".to_string()),
        },
        ToolEntry {
            name: "thulp".to_string(),
            description: "Execution context engineering platform for AI agents".to_string(),
            cargo_crate: "thulp".to_string(),
            git_repo: "https://github.com/dirmacs/thulp".to_string(),
            binary_name: Some("thulp".to_string()),
            version_check: Some("thulp --version".to_string()),
        },
        ToolEntry {
            name: "eruka".to_string(),
            description: "Context intelligence layer — schema-aware business context".to_string(),
            cargo_crate: "eruka".to_string(),
            git_repo: "https://github.com/dirmacs/eruka".to_string(),
            binary_name: Some("eruka".to_string()),
            version_check: Some("eruka --version".to_string()),
        },
        ToolEntry {
            name: "lancor".to_string(),
            description: "Rust client for llama.cpp's OpenAI-compatible API".to_string(),
            cargo_crate: "lancor".to_string(),
            git_repo: "https://github.com/dirmacs/lancor".to_string(),
            binary_name: None, // Library crate
            version_check: None,
        },
        ToolEntry {
            name: "aegis".to_string(),
            description: "System configuration manager (this tool)".to_string(),
            cargo_crate: "aegis-cli".to_string(),
            git_repo: "https://github.com/dirmacs/aegis".to_string(),
            binary_name: Some("aegis".to_string()),
            version_check: Some("aegis --version".to_string()),
        },
    ]
}

/// Look up a tool by name.
pub fn find_tool(name: &str) -> Option<ToolEntry> {
    dirmacs_registry().into_iter().find(|t| t.name == name)
}
