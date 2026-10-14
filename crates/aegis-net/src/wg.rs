//! WireGuard configuration generation and interface management.
//!
//! Generates wg-quick compatible configs from the network manifest + signed certs.
//! Uses x25519 for WireGuard keys (separate from Ed25519 CA signing keys).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine;
use rand::rngs::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

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
        Self {
            private_key,
            public_key,
        }
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
        let b64 =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .context("decoding WireGuard private key")?;
        let key_bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid key length"))?;
        Ok(StaticSecret::from(key_bytes))
    }

    /// Load public key from disk.
    pub fn load_public(path: &Path) -> Result<PublicKey> {
        let b64 =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .context("decoding WireGuard public key")?;
        let key_bytes: [u8; 32] = bytes
            .try_into()
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
    let peer = manifest
        .peer(peer_name)
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
    std::fs::write(&path, config).with_context(|| format!("writing {}", path.display()))?;

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

/// Validate and bring up a WireGuard interface from a `wg-quick` config file
/// using the provided command runner `(program, args) -> Result<Output>`.
pub fn interface_up_with<F>(config_path: &Path, mut runner: F) -> Result<()>
where
    F: FnMut(&str, &[&str]) -> Result<std::process::Output>,
{
    let meta = std::fs::metadata(config_path)
        .with_context(|| format!("stat WireGuard config {}", config_path.display()))?;
    if !meta.is_file() {
        anyhow::bail!(
            "WireGuard config {} is not a regular file",
            config_path.display()
        );
    }
    if meta.len() == 0 {
        anyhow::bail!("WireGuard config {} is empty", config_path.display());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mode = meta.mode() & 0o777;
        if mode & 0o077 != 0 {
            anyhow::bail!(
                "refusing to bring up {}: file mode {:04o} allows group/other access (expected 0600)",
                config_path.display(),
                mode
            );
        }
    }

    let path_str = config_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("config path is not valid UTF-8"))?;
    let output = runner("wg-quick", &["up", path_str])
        .with_context(|| format!("running wg-quick up {}", config_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "wg-quick up {} failed: {}",
            config_path.display(),
            stderr.trim()
        );
    }

    Ok(())
}

/// Bring up a WireGuard interface from a `wg-quick` config file via `wg-quick up`.
pub fn interface_up(config_path: &Path) -> Result<()> {
    interface_up_with(config_path, |prog, args| {
        std::process::Command::new(prog)
            .args(args)
            .output()
            .with_context(|| format!("spawning {prog}"))
    })
}

/// Tear down a WireGuard interface (by interface name or config path) using
/// the provided command runner.
pub fn interface_down_with<F>(target: &str, mut runner: F) -> Result<()>
where
    F: FnMut(&str, &[&str]) -> Result<std::process::Output>,
{
    let trimmed = target.trim();
    if trimmed.is_empty() {
        anyhow::bail!("interface or config target must not be empty");
    }
    if trimmed.starts_with('-') {
        anyhow::bail!("invalid interface or config target '{trimmed}'");
    }

    let output = runner("wg-quick", &["down", trimmed])
        .with_context(|| format!("running wg-quick down {trimmed}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("wg-quick down {trimmed} failed: {}", stderr.trim());
    }

    Ok(())
}

/// Tear down a WireGuard interface via `wg-quick down`.
pub fn interface_down(target: &str) -> Result<()> {
    interface_down_with(target, |prog, args| {
        std::process::Command::new(prog)
            .args(args)
            .output()
            .with_context(|| format!("spawning {prog}"))
    })
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
        )
        .unwrap();

        assert!(config.contains("[Interface]"));
        assert!(config.contains("10.42.0.2/32"));
        assert!(config.contains("[Peer]"));
        assert!(config.contains("1.2.3.4:51820"));
        assert!(config.contains("PersistentKeepalive"));
    }

    #[test]
    #[cfg(unix)]
    fn interface_up_rejects_group_or_world_readable_config() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("aegis0.conf");
        std::fs::write(&path, "[Interface]\nPrivateKey = AAAA\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let mut called = false;
        let err = interface_up_with(&path, |_prog, _args| {
            called = true;
            unreachable!("runner must not be called when permissions are too wide");
        })
        .unwrap_err();

        assert!(!called);
        assert!(
            format!("{err:#}").contains("0644"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn interface_up_invokes_wg_quick_on_0600_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_config("[Interface]\nPrivateKey = AAAA\n", dir.path(), "aegis0").unwrap();

        let mut recorded: Vec<(String, Vec<String>)> = Vec::new();
        interface_up_with(&path, |prog, args| {
            recorded.push((
                prog.to_string(),
                args.iter().map(|s| (*s).to_string()).collect(),
            ));
            Ok(std::process::Output {
                status: std::os::unix::process::ExitStatusExt::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, "wg-quick");
        assert_eq!(
            recorded[0].1,
            vec!["up".to_string(), path.display().to_string()]
        );
    }

    #[test]
    fn interface_down_rejects_empty_and_flag_targets() {
        for bad in ["", "   ", "-h", "--help"] {
            let err = interface_down_with(bad, |_prog, _args| {
                unreachable!("runner must not be called on invalid target");
            });
            assert!(err.is_err(), "expected error for target {bad:?}");
        }
    }
}
