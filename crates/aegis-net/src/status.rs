//! Network status and health checks.

use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::ca::{CertificateAuthority, NodeCertificate};
use crate::config::NetworkManifest;
use crate::wg;

/// Overall network health status.
#[derive(Debug)]
pub struct NetworkStatus {
    pub network_name: String,
    pub ca_valid: bool,
    pub ca_expires: Option<String>,
    pub wg_available: bool,
    pub wg_interface_up: bool,
    pub total_peers: usize,
    pub peers_with_certs: usize,
    pub expired_certs: Vec<String>,
}

/// Check overall network health.
pub fn check(manifest: &NetworkManifest, config_dir: &Path) -> NetworkStatus {
    let ca_status = check_ca(config_dir);
    let wg_available = wg::check_wg_available();
    let wg_up = check_interface_up(&manifest.network.interface);

    let mut peers_with_certs = 0;
    let mut expired_certs = Vec::new();

    for name in manifest.peers.keys() {
        let cert_path = config_dir.join("peers").join(name).join(format!("{}.cert", name));
        if cert_path.exists() {
            peers_with_certs += 1;
            if let Ok(cert) = NodeCertificate::load(&cert_path) {
                if !cert.is_valid() {
                    expired_certs.push(name.clone());
                }
            }
        }
    }

    NetworkStatus {
        network_name: manifest.network.name.clone(),
        ca_valid: ca_status.is_ok(),
        ca_expires: ca_status.ok().map(|ca| ca.ca_cert.expires_at.format("%Y-%m-%d").to_string()),
        wg_available,
        wg_interface_up: wg_up,
        total_peers: manifest.peers.len(),
        peers_with_certs,
        expired_certs,
    }
}

fn check_ca(config_dir: &Path) -> Result<CertificateAuthority> {
    let ca_dir = config_dir.join("ca");
    CertificateAuthority::load(&ca_dir)
}

fn check_interface_up(interface: &str) -> bool {
    Command::new("ip")
        .args(["link", "show", interface])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get WireGuard interface statistics (if running).
pub fn wg_stats(interface: &str) -> Option<String> {
    Command::new("wg")
        .args(["show", interface])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}
