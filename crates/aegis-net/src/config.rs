//! Declarative TOML network configuration types.

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};

/// Top-level aegis network manifest (`aegis-net.toml`).
#[derive(Debug, Deserialize, Serialize)]
pub struct NetworkManifest {
    pub network: NetworkConfig,
    #[serde(default)]
    pub lighthouse: Option<LighthouseConfig>,
    #[serde(default)]
    pub peers: HashMap<String, PeerConfig>,
    #[serde(default)]
    pub firewall: FirewallConfig,
}

/// Core network settings.
#[derive(Debug, Deserialize, Serialize)]
pub struct NetworkConfig {
    /// Human-readable network name (e.g., "dirmacs-mesh")
    pub name: String,
    /// Network CIDR (e.g., "10.42.0.0/24")
    pub cidr: Ipv4Net,
    /// Directory for CA keys, certs, peer configs
    #[serde(default = "default_config_dir")]
    pub config_dir: PathBuf,
    /// WireGuard listen port
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    /// MTU for the WireGuard interface
    #[serde(default = "default_mtu")]
    pub mtu: u16,
    /// WireGuard interface name
    #[serde(default = "default_interface")]
    pub interface: String,
}

/// Lighthouse (discovery/relay node) configuration.
#[derive(Debug, Deserialize, Serialize)]
pub struct LighthouseConfig {
    /// Whether this node is a lighthouse
    #[serde(default)]
    pub am_lighthouse: bool,
    /// Public IP/hostname of the lighthouse
    pub public_addr: Option<String>,
    /// Lighthouse's overlay IP
    pub overlay_ip: Option<IpAddr>,
    /// DNS name for lighthouse discovery
    pub dns: Option<String>,
}

/// Per-peer configuration in the manifest.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeerConfig {
    /// Overlay IP within the network CIDR
    pub ip: IpAddr,
    /// Groups this peer belongs to (used for firewall rules)
    #[serde(default)]
    pub groups: Vec<String>,
    /// Optional public endpoint for direct connection
    pub endpoint: Option<String>,
    /// Whether this peer is a lighthouse
    #[serde(default)]
    pub lighthouse: bool,
    /// Optional certificate duration override (e.g., "8760h" = 1 year)
    pub cert_duration: Option<String>,
}

/// Firewall configuration using security groups.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct FirewallConfig {
    /// Default action for unmatched traffic
    #[serde(default = "default_action")]
    pub default_action: FirewallAction,
    /// Inbound rules
    #[serde(default)]
    pub inbound: Vec<FirewallRule>,
    /// Outbound rules
    #[serde(default)]
    pub outbound: Vec<FirewallRule>,
}

/// A single firewall rule.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FirewallRule {
    /// Port or port range (e.g., "443", "8000-9000", "any")
    pub port: String,
    /// Protocol: "tcp", "udp", "icmp", "any"
    #[serde(default = "default_proto")]
    pub proto: String,
    /// Source/destination groups (e.g., ["servers", "admin"])
    #[serde(default)]
    pub groups: Vec<String>,
    /// Source/destination specific peer names
    #[serde(default)]
    pub peers: Vec<String>,
    /// Action: allow or deny
    #[serde(default = "default_allow")]
    pub action: FirewallAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FirewallAction {
    Allow,
    Deny,
}

impl Default for FirewallAction {
    fn default() -> Self {
        Self::Deny
    }
}

fn default_config_dir() -> PathBuf {
    PathBuf::from("/etc/aegis-net")
}

fn default_listen_port() -> u16 {
    51820
}

fn default_mtu() -> u16 {
    1300
}

fn default_interface() -> String {
    "aegis0".to_string()
}

fn default_action() -> FirewallAction {
    FirewallAction::Deny
}

fn default_proto() -> String {
    "any".to_string()
}

fn default_allow() -> FirewallAction {
    FirewallAction::Allow
}

impl NetworkManifest {
    /// Load from a TOML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let manifest: Self = toml::from_str(&content)
            .with_context(|| format!("parsing {}", path.display()))?;
        Ok(manifest)
    }

    /// Save to a TOML file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("serializing network manifest")?;
        std::fs::write(path, content)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    /// Get a peer by name.
    pub fn peer(&self, name: &str) -> Option<&PeerConfig> {
        self.peers.get(name)
    }

    /// Get all peers in a given group.
    pub fn peers_in_group(&self, group: &str) -> Vec<(&str, &PeerConfig)> {
        self.peers
            .iter()
            .filter(|(_, p)| p.groups.iter().any(|g| g == group))
            .map(|(name, peer)| (name.as_str(), peer))
            .collect()
    }

    /// Get the lighthouse peer, if any.
    pub fn lighthouse_peer(&self) -> Option<(&str, &PeerConfig)> {
        self.peers
            .iter()
            .find(|(_, p)| p.lighthouse)
            .map(|(name, peer)| (name.as_str(), peer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_network_manifest() {
        let toml_str = r#"
[network]
name = "dirmacs-mesh"
cidr = "10.42.0.0/24"

[peers.vps]
ip = "10.42.0.1"
groups = ["servers"]
lighthouse = true
endpoint = "217.216.78.38:51820"

[peers.baala-laptop]
ip = "10.42.0.2"
groups = ["admin", "dev"]

[[firewall.inbound]]
port = "any"
groups = ["admin"]
action = "allow"

[[firewall.inbound]]
port = "443"
proto = "tcp"
groups = ["servers"]
action = "allow"
"#;
        let manifest: NetworkManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.network.name, "dirmacs-mesh");
        assert_eq!(manifest.peers.len(), 2);
        assert_eq!(manifest.firewall.inbound.len(), 2);

        let (name, peer) = manifest.lighthouse_peer().unwrap();
        assert_eq!(name, "vps");
        assert!(peer.lighthouse);

        let admins = manifest.peers_in_group("admin");
        assert_eq!(admins.len(), 1);
        assert_eq!(admins[0].0, "baala-laptop");
    }

    #[test]
    fn default_values() {
        let toml_str = r#"
[network]
name = "test"
cidr = "10.0.0.0/24"
"#;
        let manifest: NetworkManifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.network.listen_port, 51820);
        assert_eq!(manifest.network.mtu, 1300);
        assert_eq!(manifest.network.interface, "aegis0");
        assert_eq!(manifest.firewall.default_action, FirewallAction::Deny);
    }

    #[test]
    fn roundtrip_serialize() {
        let toml_str = r#"
[network]
name = "test"
cidr = "10.0.0.0/24"

[peers.node1]
ip = "10.0.0.1"
groups = ["servers"]
"#;
        let manifest: NetworkManifest = toml::from_str(toml_str).unwrap();
        let serialized = toml::to_string_pretty(&manifest).unwrap();
        let reparsed: NetworkManifest = toml::from_str(&serialized).unwrap();
        assert_eq!(manifest.network.name, reparsed.network.name);
        assert_eq!(manifest.peers.len(), reparsed.peers.len());
    }
}
