//! Node inventory — environment discovery and cross-node synchronization.
//!
//! Each node in the aegis-net mesh publishes a snapshot of its environment
//! (OS, tools, repos, services, models) to a shared inventory format.
//! Nodes can pull each other's inventories over SSH to build a unified
//! view of the dirmacs fleet.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A complete snapshot of a node's environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInventory {
    /// Node name from aegis-net manifest
    pub node: String,
    /// Overlay IP (e.g. 10.42.0.3)
    pub overlay_ip: String,
    /// When this snapshot was taken (ISO 8601)
    pub timestamp: String,
    /// OS and hardware info
    pub system: SystemInfo,
    /// Installed dirmacs tools
    pub tools: Vec<ToolInfo>,
    /// Git repositories
    pub repos: Vec<RepoInfo>,
    /// Running services
    #[serde(default)]
    pub services: Vec<ServiceInfo>,
    /// Cached LLM models
    #[serde(default)]
    pub models: Vec<ModelInfo>,
    /// Shell and path configuration
    #[serde(default)]
    pub shell: ShellInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub kernel: String,
    pub cpu: String,
    pub memory_gb: f64,
    pub disk_total_gb: f64,
    pub disk_avail_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub last_commit: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub status: String,
    pub port: Option<u16>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShellInfo {
    pub default_shell: String,
    pub path_dirs: Vec<String>,
}

/// Inventory diff between two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryDiff {
    pub node_a: String,
    pub node_b: String,
    pub tools_only_a: Vec<String>,
    pub tools_only_b: Vec<String>,
    pub tools_both: Vec<String>,
    pub repos_only_a: Vec<String>,
    pub repos_only_b: Vec<String>,
    pub repos_both: Vec<String>,
    pub services_only_a: Vec<String>,
    pub services_only_b: Vec<String>,
}

impl NodeInventory {
    /// Save inventory to a TOML file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("serializing inventory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Load inventory from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        toml::from_str(&content)
            .with_context(|| format!("parsing {}", path.display()))
    }

    /// Diff two inventories.
    pub fn diff(&self, other: &NodeInventory) -> InventoryDiff {
        let a_tools: Vec<String> = self.tools.iter().map(|t| t.name.clone()).collect();
        let b_tools: Vec<String> = other.tools.iter().map(|t| t.name.clone()).collect();
        let a_repos: Vec<String> = self.repos.iter().map(|r| r.name.clone()).collect();
        let b_repos: Vec<String> = other.repos.iter().map(|r| r.name.clone()).collect();
        let a_services: Vec<String> = self.services.iter().map(|s| s.name.clone()).collect();
        let b_services: Vec<String> = other.services.iter().map(|s| s.name.clone()).collect();

        InventoryDiff {
            node_a: self.node.clone(),
            node_b: other.node.clone(),
            tools_only_a: a_tools.iter().filter(|t| !b_tools.contains(t)).cloned().collect(),
            tools_only_b: b_tools.iter().filter(|t| !a_tools.contains(t)).cloned().collect(),
            tools_both: a_tools.iter().filter(|t| b_tools.contains(t)).cloned().collect(),
            repos_only_a: a_repos.iter().filter(|r| !b_repos.contains(r)).cloned().collect(),
            repos_only_b: b_repos.iter().filter(|r| !a_repos.contains(r)).cloned().collect(),
            repos_both: a_repos.iter().filter(|r| b_repos.contains(r)).cloned().collect(),
            services_only_a: a_services.iter().filter(|s| !b_services.contains(s)).cloned().collect(),
            services_only_b: b_services.iter().filter(|s| !a_services.contains(s)).cloned().collect(),
        }
    }
}

/// Print a formatted diff between two inventories.
pub fn print_diff(diff: &InventoryDiff) {
    println!("Inventory diff: {} ↔ {}\n", diff.node_a, diff.node_b);

    println!("Tools:");
    for t in &diff.tools_both { println!("  = {}", t); }
    for t in &diff.tools_only_a { println!("  + {} (only on {})", t, diff.node_a); }
    for t in &diff.tools_only_b { println!("  + {} (only on {})", t, diff.node_b); }

    println!("\nRepos:");
    for r in &diff.repos_both { println!("  = {}", r); }
    for r in &diff.repos_only_a { println!("  + {} (only on {})", r, diff.node_a); }
    for r in &diff.repos_only_b { println!("  + {} (only on {})", r, diff.node_b); }

    println!("\nServices:");
    for s in &diff.services_only_a { println!("  {} → {} only", s, diff.node_a); }
    for s in &diff.services_only_b { println!("  {} → {} only", s, diff.node_b); }
}

/// Discover the local environment and build an inventory.
pub fn discover_local(node_name: &str, overlay_ip: &str) -> Result<NodeInventory> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_default();

    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    // Build tool list by checking PATH
    let dirmacs_tools = [
        "aegis", "lancor", "nimakai", "pawan", "daedra", "doltclaw", "thulp",
        "llama-server", "llama-cli", "wg",
        "cargo", "rustc", "node", "bun", "deno", "go", "nim", "zig", "odin",
        "python3", "python", "docker", "kubectl", "ollama",
        "starship", "bat", "eza", "fd", "rg", "fzf", "zoxide", "delta",
        "gh", "git",
    ];

    let mut tools = Vec::new();
    for name in &dirmacs_tools {
        if let Ok(path) = which::which(name) {
            let version = get_tool_version(name);
            tools.push(ToolInfo {
                name: name.to_string(),
                version,
                path: path.to_string_lossy().to_string(),
            });
        }
    }

    // Find git repos in common locations — platform-aware
    let mut repos = Vec::new();
    let search_dirs: Vec<PathBuf> = if cfg!(target_os = "windows") {
        let mut dirs = Vec::new();
        // Common Windows dev directories
        for drive in &["C:\\Development", "D:\\Development", "C:\\Users"] {
            let p = PathBuf::from(drive);
            if p.exists() {
                dirs.push(p);
            }
        }
        if let Some(home) = dirs::home_dir() {
            dirs.push(home);
        }
        dirs
    } else if cfg!(target_os = "macos") {
        vec![
            dirs::home_dir().unwrap_or_default(),
            PathBuf::from("/tmp"),
            PathBuf::from("/opt"),
        ]
    } else {
        vec![
            PathBuf::from("/opt"),
            dirs::home_dir().unwrap_or_default(),
        ]
    };

    for base in &search_dirs {
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let git_dir = entry.path().join(".git");
                if git_dir.exists() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let branch = git_branch(&entry.path());
                    let dirty = git_dirty(&entry.path());
                    repos.push(RepoInfo {
                        name,
                        path: entry.path().to_string_lossy().to_string(),
                        branch,
                        last_commit: String::new(),
                        dirty,
                    });
                }
            }
        }
    }

    // Shell info — use correct PATH separator per platform
    let path_var = std::env::var("PATH").unwrap_or_default();
    let separator = if cfg!(target_os = "windows") { ';' } else { ':' };
    let path_dirs: Vec<String> = path_var.split(separator).map(|s| s.to_string()).collect();
    let default_shell = std::env::var("SHELL")
        .or_else(|_| std::env::var("COMSPEC"))
        .unwrap_or_default();

    // Discover Ollama models
    let models = discover_ollama_models();

    // Discover running services
    let services = discover_services();

    Ok(NodeInventory {
        node: node_name.to_string(),
        overlay_ip: overlay_ip.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        system: SystemInfo {
            hostname,
            os,
            arch,
            kernel: String::new(),
            cpu: String::new(),
            memory_gb: 0.0,
            disk_total_gb: 0.0,
            disk_avail_gb: 0.0,
        },
        tools,
        repos,
        services,
        models,
        shell: ShellInfo { default_shell, path_dirs },
    })
}

/// Get a tool's version by running common version flags.
fn get_tool_version(name: &str) -> String {
    let flag = match name {
        "go" => "version",
        "zig" => "version",
        _ => "--version",
    };
    std::process::Command::new(name)
        .arg(flag)
        .output()
        .ok()
        .and_then(|o| {
            let out = String::from_utf8_lossy(&o.stdout).to_string();
            let first = out.lines().next().unwrap_or("").trim().to_string();
            if first.is_empty() { None } else { Some(first) }
        })
        .unwrap_or_default()
}

/// Get current git branch for a repo.
fn git_branch(repo: &Path) -> String {
    std::process::Command::new("git")
        .args(["-C", &repo.to_string_lossy(), "branch", "--show-current"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Check if a git repo has uncommitted changes.
fn git_dirty(repo: &Path) -> bool {
    std::process::Command::new("git")
        .args(["-C", &repo.to_string_lossy(), "status", "--porcelain"])
        .output()
        .ok()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Discover locally available Ollama models.
fn discover_ollama_models() -> Vec<ModelInfo> {
    let output = match std::process::Command::new("ollama").arg("list").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };

    output
        .lines()
        .skip(1) // header
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[0].to_string();
                let size_str = parts.get(2).unwrap_or(&"0");
                let size_bytes = parse_size(size_str);
                Some(ModelInfo {
                    name,
                    path: String::new(),
                    size_bytes,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Parse human-readable size like "1.9 GB" or "639 MB" to bytes.
fn parse_size(s: &str) -> u64 {
    let s = s.trim();
    if let Ok(n) = s.parse::<f64>() {
        // Assume GB if no unit
        (n * 1_073_741_824.0) as u64
    } else {
        0
    }
}

/// Discover running services by checking common ports/processes.
fn discover_services() -> Vec<ServiceInfo> {
    let mut services = Vec::new();
    let checks = [
        ("ollama", 11434, "ollama"),
        ("docker", 2375, "docker"),
    ];

    for (name, port, binary) in &checks {
        if which::which(binary).is_ok() {
            services.push(ServiceInfo {
                name: name.to_string(),
                status: "available".to_string(),
                port: Some(*port),
                url: None,
            });
        }
    }
    services
}

/// Pull inventory from a remote node via SSH.
pub fn pull_remote(user: &str, host: &str, inventory_path: &str) -> Result<NodeInventory> {
    let output = std::process::Command::new("ssh")
        .args([
            "-o", "ConnectTimeout=10",
            &format!("{}@{}", user, host),
            &format!("cat {}", inventory_path),
        ])
        .output()
        .context("SSH to remote node")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("SSH failed: {}", stderr);
    }

    let content = String::from_utf8_lossy(&output.stdout);
    toml::from_str(&content).context("parsing remote inventory")
}

/// Push local inventory to a remote node via SSH.
pub fn push_to_remote(
    inventory: &NodeInventory,
    user: &str,
    host: &str,
    remote_path: &str,
) -> Result<()> {
    let content = toml::to_string_pretty(inventory)
        .context("serializing inventory")?;

    let output = std::process::Command::new("ssh")
        .args([
            "-o", "ConnectTimeout=10",
            &format!("{}@{}", user, host),
            &format!("mkdir -p $(dirname {}) && cat > {}", remote_path, remote_path),
        ])
        .stdin(std::process::Stdio::piped())
        .output()
        .context("SSH push")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("SSH push failed: {}", stderr);
    }

    Ok(())
}
