//! Peer management — add, remove, list nodes in the overlay network.

use std::net::IpAddr;
use std::path::Path;

use anyhow::{Result, bail};

use crate::ca::{CertificateAuthority, NodeCertificate, NodeKeypair};
use crate::config::{NetworkManifest, PeerConfig};

/// Add a new peer to the network manifest and sign its certificate.
pub fn add_peer(
    manifest: &mut NetworkManifest,
    ca: &CertificateAuthority,
    name: &str,
    ip: IpAddr,
    groups: Vec<String>,
    endpoint: Option<String>,
    lighthouse: bool,
    cert_duration_days: u32,
    config_dir: &Path,
) -> Result<NodeCertificate> {
    // Check for duplicate name
    if manifest.peers.contains_key(name) {
        bail!("peer '{}' already exists", name);
    }

    // Check for duplicate IP
    if manifest.peers.values().any(|p| p.ip == ip) {
        bail!("IP {} is already assigned to another peer", ip);
    }

    // Verify IP is within network CIDR
    if let IpAddr::V4(v4) = ip {
        if !manifest.network.cidr.contains(&v4) {
            bail!(
                "IP {} is not within network CIDR {}",
                ip, manifest.network.cidr
            );
        }
    }

    // Generate node keypair
    let (node_signing_key, node_verifying_key) = NodeKeypair::generate();

    // Sign certificate
    let cert = ca.sign_node(
        name,
        &ip.to_string(),
        &groups,
        &node_verifying_key.to_bytes(),
        cert_duration_days,
    )?;

    // Save node key and cert
    let peer_dir = config_dir.join("peers").join(name);
    NodeKeypair::save_key(&node_signing_key, &peer_dir, name)?;
    cert.save(&peer_dir)?;

    // Add to manifest
    manifest.peers.insert(
        name.to_string(),
        PeerConfig {
            ip,
            groups,
            endpoint,
            lighthouse,
            cert_duration: Some(format!("{}d", cert_duration_days)),
        },
    );

    tracing::info!("Added peer '{}' ({})", name, ip);
    Ok(cert)
}

/// Remove a peer from the network manifest.
pub fn remove_peer(manifest: &mut NetworkManifest, name: &str) -> Result<PeerConfig> {
    let peer = manifest
        .peers
        .remove(name)
        .ok_or_else(|| anyhow::anyhow!("peer '{}' not found", name))?;

    tracing::info!("Removed peer '{}' ({})", name, peer.ip);
    Ok(peer)
}

/// List all peers with their status.
pub fn list_peers(manifest: &NetworkManifest) -> Vec<PeerSummary> {
    manifest
        .peers
        .iter()
        .map(|(name, peer)| PeerSummary {
            name: name.clone(),
            ip: peer.ip,
            groups: peer.groups.clone(),
            endpoint: peer.endpoint.clone(),
            lighthouse: peer.lighthouse,
        })
        .collect()
}

/// Summary of a peer for display.
#[derive(Debug)]
pub struct PeerSummary {
    pub name: String,
    pub ip: IpAddr,
    pub groups: Vec<String>,
    pub endpoint: Option<String>,
    pub lighthouse: bool,
}

/// Find the next available IP in the network CIDR.
pub fn next_available_ip(manifest: &NetworkManifest) -> Option<IpAddr> {
    let network = manifest.network.cidr;
    let used_ips: Vec<IpAddr> = manifest.peers.values().map(|p| p.ip).collect();

    for host in network.hosts() {
        let ip = IpAddr::V4(host);
        if !used_ips.contains(&ip) {
            return Some(ip);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn test_manifest() -> NetworkManifest {
        let toml_str = r#"
[network]
name = "test"
cidr = "10.42.0.0/24"
"#;
        toml::from_str(toml_str).unwrap()
    }

    #[test]
    fn next_ip_empty_network() {
        let manifest = test_manifest();
        let ip = next_available_ip(&manifest).unwrap();
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 42, 0, 1)));
    }

    #[test]
    fn next_ip_with_peers() {
        let mut manifest = test_manifest();
        manifest.peers.insert("node1".to_string(), PeerConfig {
            ip: IpAddr::V4(Ipv4Addr::new(10, 42, 0, 1)),
            groups: vec![],
            endpoint: None,
            lighthouse: false,
            cert_duration: None,
        });
        let ip = next_available_ip(&manifest).unwrap();
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 42, 0, 2)));
    }

    #[test]
    fn list_peers_empty() {
        let manifest = test_manifest();
        assert!(list_peers(&manifest).is_empty());
    }
}
