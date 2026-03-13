use std::process::Command;

use serde::Serialize;
use tracing::debug;

use crate::registry::ToolEntry;

/// Health status for a single tool.
#[derive(Debug, Serialize)]
pub struct ToolHealth {
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
    pub binary_path: Option<String>,
}

/// Check the health of a single tool.
pub fn check_tool(tool: &ToolEntry) -> ToolHealth {
    let binary_name = tool
        .binary_name
        .as_deref()
        .unwrap_or(&tool.name);

    let binary_path = which::which(binary_name)
        .ok()
        .map(|p| p.display().to_string());

    let installed = binary_path.is_some();

    let version = if installed {
        get_version(tool)
    } else {
        None
    };

    debug!(
        "tool {} — installed: {}, version: {:?}",
        tool.name, installed, version
    );

    ToolHealth {
        name: tool.name.clone(),
        installed,
        version,
        binary_path,
    }
}

/// Check health of all dirmacs tools.
pub fn check_all() -> Vec<ToolHealth> {
    crate::registry::dirmacs_registry()
        .iter()
        .filter(|t| t.binary_name.is_some()) // Skip library-only crates
        .map(check_tool)
        .collect()
}

fn get_version(tool: &ToolEntry) -> Option<String> {
    let check = tool.version_check.as_ref()?;
    let parts: Vec<&str> = check.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }
    let output = Command::new(parts[0]).args(&parts[1..]).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stdout.trim().is_empty() {
        stderr.to_string()
    } else {
        stdout.to_string()
    };
    Some(text.trim().to_string())
}
