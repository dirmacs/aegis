//! WireGuard configuration generation and interface management.
//!
//! Generates wg-quick compatible configs from the network manifest + signed certs.
//! Uses x25519 for WireGuard keys (separate from Ed25519 CA signing keys).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine;
use x25519_dalek::{StaticSecret, PublicKey};
use rand::rngs::OsRng;

use crate::config::NetworkManifest;
use crate::firewall;

/// A WireGuard keypair (Curve25519).
pub struct WgKeypair {
    pub private_key: StaticSecret,
    pub public_key: PublicKey,
}

impl std::fmt::Debug for WgKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WgKeypair")
            .field("public_key", &self.public_key_base64())
            .field("private_key", &"[redacted]")
            .finish()
    }
}

impl WgKeypair {
    /// Generate a new WireGuard keypair.
    pub fn generate() -> Self {
        let private_key = StaticSecret::random_from_rng(OsRng);
        let public_key = PublicKey::from(&private_key);
        Self { private_key, public_key }
    }

    /// Encode private key as base64 (for wg-quick config).
    pub fn private_key_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.private_key.to_bytes())
    }

    /// Encode public key as base64.
    pub fn public_key_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.public_key.as_bytes())
    }

    /// Save keypair to disk.
    pub fn save(&self, dir: &Path, name: &str) -> Result<()> {
        std::fs::create_dir_all(dir)?;

        let priv_path = dir.join(format!("{}.wg.key", name));
        std::fs::write(&priv_path, self.private_key_base64())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&priv_path, std::fs::Permissions::from_mode(0o600))?;
        }

        let pub_path = dir.join(format!("{}.wg.pub", name));
        std::fs::write(&pub_path, self.public_key_base64())?;

        Ok(())
    }

    /// Load private key from disk.
    pub fn load_private(path: &Path) -> Result<StaticSecret> {
        let b64 = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim())
            .context("decoding WireGuard private key")?;
        let key_bytes: [u8; 32] = bytes.try_into()
            .map_err(|_| anyhow::anyhow!("invalid key length"))?;
        Ok(StaticSecret::from(key_bytes))
    }

    /// Load public key from disk.
    pub fn load_public(path: &Path) -> Result<PublicKey> {
        let b64 = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim())
            .context("decoding WireGuard public key")?;
        let key_bytes: [u8; 32] = bytes.try_into()
            .map_err(|_| anyhow::anyhow!("invalid key length"))?;
        Ok(PublicKey::from(key_bytes))
    }
}

/// Generate a wg-quick compatible config for a peer.
pub fn generate_config(
    manifest: &NetworkManifest,
    peer_name: &str,
    private_key_b64: &str,
    peer_public_keys: &std::collections::HashMap<String, String>,
) -> Result<String> {
    let peer = manifest.peer(peer_name)
        .ok_or_else(|| anyhow::anyhow!("peer '{}' not found in manifest", peer_name))?;

    let mut config = String::new();

    // [Interface] section
    config.push_str("[Interface]\n");
    config.push_str(&format!("# {}\n", peer_name));
    config.push_str(&format!("PrivateKey = {}\n", private_key_b64));
    config.push_str(&format!("Address = {}/32\n", peer.ip));
    config.push_str(&format!("ListenPort = {}\n", manifest.network.listen_port));
    config.push_str(&format!("MTU = {}\n", manifest.network.mtu));
    config.push('\n');

    // [Peer] sections for each allowed peer
    let allowed_ips = firewall::resolve_allowed_peers(manifest, peer_name);

    for (name, other_peer) in &manifest.peers {
        if name == peer_name {
            continue;
        }

        let other_ip = format!("{}/32", other_peer.ip);
        if !allowed_ips.contains(&other_ip) && !other_peer.lighthouse {
            continue;
        }

        let pub_key = match peer_public_keys.get(name.as_str()) {
            Some(k) => k,
            None => {
                tracing::warn!("no public key for peer '{}', skipping", name);
                continue;
            }
        };

        config.push_str("[Peer]\n");
        config.push_str(&format!("# {}\n", name));
        config.push_str(&format!("PublicKey = {}\n", pub_key));

        // AllowedIPs
        config.push_str(&format!("AllowedIPs = {}/32\n", other_peer.ip));

        // Endpoint (only for peers with known public addresses)
        if let Some(ref endpoint) = other_peer.endpoint {
            config.push_str(&format!("Endpoint = {}\n", endpoint));
        }

        // Persistent keepalive for NAT traversal
        if other_peer.lighthouse || peer.lighthouse {
            config.push_str("PersistentKeepalive = 25\n");
        }

        config.push('\n');
    }

    Ok(config)
}

/// Write a wg-quick config file to disk.
pub fn write_config(config: &str, dir: &Path, interface: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.conf", interface));
    std::fs::write(&path, config)
        .with_context(|| format!("writing {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(path)
}

/// Check if WireGuard tools are available on the system.
pub fn check_wg_available() -> bool {
    which::which("wg").is_ok() && which::which("wg-quick").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_keypair() {
        let kp = WgKeypair::generate();
        assert_eq!(kp.private_key_base64().len(), 44); // base64 of 32 bytes
        assert_eq!(kp.public_key_base64().len(), 44);
    }

    #[test]
    fn keypair_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let kp = WgKeypair::generate();
        kp.save(dir.path(), "test").unwrap();

        let loaded_priv = WgKeypair::load_private(&dir.path().join("test.wg.key")).unwrap();
        let loaded_pub = WgKeypair::load_public(&dir.path().join("test.wg.pub")).unwrap();

        assert_eq!(kp.private_key.to_bytes(), loaded_priv.to_bytes());
        assert_eq!(kp.public_key.as_bytes(), loaded_pub.as_bytes());
    }

    #[test]
    fn generate_wg_config() {
        let toml_str = r#"
[network]
name = "test"
cidr = "10.42.0.0/24"

[peers.vps]
ip = "10.42.0.1"
groups = ["servers"]
lighthouse = true
endpoint = "1.2.3.4:51820"

[peers.laptop]
ip = "10.42.0.2"
groups = ["admin"]

[[firewall.inbound]]
port = "any"
groups = ["admin"]
action = "allow"

[[firewall.inbound]]
port = "any"
groups = ["servers"]
action = "allow"
"#;
        let manifest: NetworkManifest = toml::from_str(toml_str).unwrap();

        let mut pub_keys = std::collections::HashMap::new();
        let vps_kp = WgKeypair::generate();
        let laptop_kp = WgKeypair::generate();
        pub_keys.insert("vps".to_string(), vps_kp.public_key_base64());
        pub_keys.insert("laptop".to_string(), laptop_kp.public_key_base64());

        let config = generate_config(
            &manifest,
            "laptop",
            &laptop_kp.private_key_base64(),
            &pub_keys,
        ).unwrap();

        assert!(config.contains("[Interface]"));
        assert!(config.contains("10.42.0.2/32"));
        assert!(config.contains("[Peer]"));
        assert!(config.contains("1.2.3.4:51820"));
        assert!(config.contains("PersistentKeepalive"));
    }
}
