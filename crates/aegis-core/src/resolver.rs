//! Agentic package install resolver.
//!
//! For each package, an LLM agent inspects the live environment and reasons
//! about the best installation method. The agent can try, fail, and retry
//! with alternative strategies.
//!
//! Resolution cascade: cache (learned) → LLM agent → cargo fallback.
//! No hardcoded heuristic tables — the LLM is the source of truth.
//! Successful resolutions are cached so the LLM is only consulted once
//! per (package, platform) pair.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};

use crate::package::InstallMethod;

/// A resolved installation instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedInstall {
    pub method: InstallMethod,
    /// The package name for this specific manager
    pub manager_package: String,
    /// Raw shell command if method is Script
    pub script: Option<String>,
    /// How this was resolved
    pub source: ResolutionSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResolutionSource {
    LlmAgent { model: String, attempts: u32 },
    Cache,
    Explicit,
    CargoFallback,
}

/// System context fed to the agent for reasoning.
#[derive(Debug, Clone, Serialize)]
pub struct SystemContext {
    pub os: String,
    pub arch: String,
    pub distro: String,
    pub available_managers: Vec<String>,
    pub installed_tools: Vec<String>,
}

impl SystemContext {
    /// Discover the current system context.
    pub fn discover() -> Self {
        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();
        let distro = detect_distro();

        let mut available_managers = Vec::new();
        for mgr in &["apt-get", "scoop", "winget", "brew", "cargo", "snap", "pip3", "npm"] {
            if which::which(mgr).is_ok() {
                available_managers.push(mgr.to_string());
            }
        }

        let mut installed_tools = Vec::new();
        for tool in &["git", "curl", "wget", "python3", "node", "cargo", "go", "docker"] {
            if which::which(tool).is_ok() {
                installed_tools.push(tool.to_string());
            }
        }

        Self { os, arch, distro, available_managers, installed_tools }
    }
}

/// Persistent resolution cache — learned knowledge, not hardcoded.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ResolveCache {
    pub entries: HashMap<String, ResolvedInstall>,
}

impl ResolveCache {
    pub fn load() -> Self {
        let path = cache_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| toml::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = cache_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn get(&self, package: &str) -> Option<&ResolvedInstall> {
        let key = cache_key(package);
        self.entries.get(&key)
    }

    pub fn insert(&mut self, package: &str, resolved: ResolvedInstall) {
        let key = cache_key(package);
        self.entries.insert(key, resolved);
    }

    /// Remove a cached entry (e.g., when the cached method fails at install time).
    pub fn invalidate(&mut self, package: &str) {
        let key = cache_key(package);
        self.entries.remove(&key);
    }
}

fn cache_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("aegis")
        .join("resolve-cache.toml")
}

fn cache_key(package: &str) -> String {
    format!("{}:{}:{}", package, std::env::consts::OS, std::env::consts::ARCH)
}

/// Detect available package managers on this system.
pub fn detect_managers() -> Vec<InstallMethod> {
    let mut managers = Vec::new();
    let checks: &[(&str, InstallMethod)] = &[
        ("scoop", InstallMethod::Scoop),
        ("winget", InstallMethod::Winget),
        ("apt-get", InstallMethod::Apt),
        ("cargo", InstallMethod::Cargo),
    ];
    for (binary, method) in checks {
        if which::which(binary).is_ok() {
            managers.push(*method);
        }
    }
    managers
}

/// The agentic resolver. Asks an LLM to reason about how to install a package,
/// given full system context. Can retry with error feedback.
pub fn agent_resolve(
    package: &str,
    description: &str,
    sys: &SystemContext,
    previous_error: Option<&str>,
) -> Option<ResolvedInstall> {
    let ollama_url = std::env::var("AEGIS_OLLAMA_URL")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());

    let model = std::env::var("AEGIS_RESOLVE_MODEL")
        .unwrap_or_else(|_| "qwen3.5:0.8b".to_string());

    let retry_context = if let Some(err) = previous_error {
        format!(
            "\n\nIMPORTANT: A previous install attempt FAILED with this error:\n{err}\n\
             You MUST suggest a DIFFERENT approach this time."
        )
    } else {
        String::new()
    };

    let prompt = format!(
        r#"You are a system package installation agent. Your task: determine the single best shell command to install the tool "{package}" on this system.

SYSTEM:
- OS: {os}
- Arch: {arch}
- Distro: {distro}
- Available package managers: {managers}
- Already installed: {installed}

PACKAGE:
- Name: {package}
- Description: {description}{retry_context}

RULES:
1. Use the system's native package manager when possible (apt-get on Debian/Ubuntu, scoop on Windows, brew on macOS)
2. Fall back to cargo install if the package is a Rust tool and native repos don't have it
3. If neither works, provide a curl/wget one-liner to install from GitHub releases or official install script
4. NEVER suggest building from source unless there is truly no other option

Reply with EXACTLY this format, nothing else:
METHOD: <apt|scoop|winget|cargo|script>
PACKAGE: <the package name for that manager>
COMMAND: <the full shell command>

Example:
METHOD: apt
PACKAGE: fd-find
COMMAND: sudo apt-get install -y fd-find"#,
        os = sys.os,
        arch = sys.arch,
        distro = sys.distro,
        managers = sys.available_managers.join(", "),
        installed = sys.installed_tools.join(", "),
    );

    debug!("agent prompt for '{package}': {prompt}");
    info!("agent resolving '{package}'...");

    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "options": {
            "temperature": 0.1,
            "num_predict": 128,
        }
    });

    let output = Command::new("curl")
        .args([
            "-s", "--connect-timeout", "5", "--max-time", "60",
            "-X", "POST",
            &format!("{ollama_url}/api/generate"),
            "-H", "Content-Type: application/json",
            "-d", &body.to_string(),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        warn!("Ollama query failed for '{package}'");
        return None;
    }

    let response: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let answer = response.get("response")?.as_str()?.trim().to_string();

    if answer.is_empty() {
        return None;
    }

    debug!("agent response for '{package}': {answer}");

    // Parse structured response
    parse_agent_response(&answer, &model)
}

/// Parse the agent's structured response into a ResolvedInstall.
fn parse_agent_response(response: &str, model: &str) -> Option<ResolvedInstall> {
    let mut method_str = None;
    let mut package_str = None;
    let mut command_str = None;

    for line in response.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("METHOD:") {
            method_str = Some(val.trim().to_lowercase());
        } else if let Some(val) = line.strip_prefix("PACKAGE:") {
            package_str = Some(val.trim().to_string());
        } else if let Some(val) = line.strip_prefix("COMMAND:") {
            command_str = Some(val.trim().to_string());
        }
    }

    let method_s = method_str?;
    let pkg = package_str?;
    let cmd = command_str;

    let method = match method_s.as_str() {
        "apt" | "apt-get" => InstallMethod::Apt,
        "scoop" => InstallMethod::Scoop,
        "winget" => InstallMethod::Winget,
        "cargo" => InstallMethod::Cargo,
        "script" | "curl" | "wget" | "shell" => InstallMethod::Script,
        _ => {
            warn!("agent returned unknown method '{method_s}'");
            return None;
        }
    };

    let script = if method == InstallMethod::Script {
        cmd.clone()
    } else {
        None
    };

    Some(ResolvedInstall {
        method,
        manager_package: pkg,
        script,
        source: ResolutionSource::LlmAgent {
            model: model.to_string(),
            attempts: 1,
        },
    })
}

/// Main resolution function.
///
/// Cascade: cache → LLM agent → cargo fallback.
/// If the cached method previously failed (invalidated), skips cache.
pub fn resolve(
    package: &str,
    description: &str,
    cache: &mut ResolveCache,
) -> Option<ResolvedInstall> {
    // 1. Check cache (learned from previous LLM resolutions)
    if let Some(cached) = cache.get(package) {
        info!("cache hit for '{package}' → {:?} ({})", cached.method, cached.manager_package);
        return Some(ResolvedInstall {
            source: ResolutionSource::Cache,
            ..cached.clone()
        });
    }

    // 2. LLM agent — primary resolver
    let sys = SystemContext::discover();
    if let Some(resolved) = agent_resolve(package, description, &sys, None) {
        info!(
            "agent resolved '{package}' → {:?} ({})",
            resolved.method, resolved.manager_package
        );
        cache.insert(package, resolved.clone());
        return Some(resolved);
    }

    // 3. Cargo fallback — if cargo is available and package name looks like a crate
    if which::which("cargo").is_ok() {
        info!("falling back to cargo install for '{package}'");
        let resolved = ResolvedInstall {
            method: InstallMethod::Cargo,
            manager_package: package.to_string(),
            script: None,
            source: ResolutionSource::CargoFallback,
        };
        cache.insert(package, resolved.clone());
        return Some(resolved);
    }

    warn!(
        "could not resolve install method for '{package}' on {}/{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    None
}

/// Resolve with retry — if the first install attempt fails, ask the agent
/// again with the error message so it can suggest an alternative.
pub fn resolve_with_retry(
    package: &str,
    description: &str,
    cache: &mut ResolveCache,
    error: &str,
) -> Option<ResolvedInstall> {
    // Invalidate the cached (failed) method
    cache.invalidate(package);

    let sys = SystemContext::discover();
    if let Some(resolved) = agent_resolve(package, description, &sys, Some(error)) {
        info!(
            "agent retry resolved '{package}' → {:?} ({})",
            resolved.method, resolved.manager_package
        );
        cache.insert(package, resolved.clone());
        return Some(resolved);
    }

    None
}

/// Detect Linux distro name.
fn detect_distro() -> String {
    if cfg!(target_os = "windows") {
        return "Windows".to_string();
    }
    if cfg!(target_os = "macos") {
        return "macOS".to_string();
    }
    // Try /etc/os-release
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|l| l.starts_with("PRETTY_NAME="))
                .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "Linux".to_string())
}
