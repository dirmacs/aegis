//! Peer bundle assembly and distribution for the overlay network.
//!
//! Verifies that a peer's certificate is unexpired and signed by the network CA
//! before packaging its `wg-quick` config, node certificate, and CA certificate
//! for local output or remote SSH deployment.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::Engine;

use crate::ca::{CertificateAuthority, NodeCertificate};
use crate::config::NetworkManifest;
use crate::firewall;
use crate::wg::{self, WgKeypair};

/// Verified configuration bundle for a single overlay peer.
#[derive(Debug, Clone)]
pub struct PeerBundle {
    pub peer_name: String,
    pub interface: String,
    pub wg_config: String,
    pub nftables_ruleset: String,
    pub node_cert_json: String,
    pub ca_cert_json: String,
}

/// Assemble and verify the configuration bundle for `peer_name`.
///
/// Fails closed if:
/// - `peer_name` is not in the manifest,
/// - the CA cannot be loaded,
/// - the peer's certificate is missing, expired, or fails CA signature verification,
/// - the peer's WireGuard private key is missing.
pub fn build_peer_bundle(
    manifest: &NetworkManifest,
    config_dir: &Path,
    peer_name: &str,
) -> Result<PeerBundle> {
    if manifest.peer(peer_name).is_none() {
        bail!("peer '{peer_name}' not found in manifest");
    }

    let ca_dir = config_dir.join("ca");
    let ca = CertificateAuthority::load(&ca_dir)
        .with_context(|| format!("loading CA from {}", ca_dir.display()))?;

    let peer_dir = config_dir.join("peers").join(peer_name);
    let cert_path = peer_dir.join(format!("{peer_name}.cert"));
    let cert = NodeCertificate::load(&cert_path)
        .with_context(|| format!("loading certificate for peer '{peer_name}'"))?;

    ca.verify(&cert)
        .with_context(|| format!("verifying certificate for peer '{peer_name}'"))?;

    let priv_path = peer_dir.join(format!("{peer_name}.wg.key"));
    let priv_key = std::fs::read_to_string(&priv_path)
        .with_context(|| format!("reading WireGuard private key {}", priv_path.display()))?;
    let priv_key = priv_key.trim().to_string();
    if priv_key.is_empty() {
        bail!("WireGuard private key for '{peer_name}' is empty");
    }

    let mut pub_keys: HashMap<String, String> = HashMap::new();
    for name in manifest.peers.keys() {
        let pub_path = config_dir
            .join("peers")
            .join(name)
            .join(format!("{name}.wg.pub"));
        if let Ok(key) = WgKeypair::load_public(&pub_path) {
            pub_keys.insert(
                name.clone(),
                base64::engine::general_purpose::STANDARD.encode(key.as_bytes()),
            );
        }
    }

    let wg_config = wg::generate_config(manifest, peer_name, &priv_key, &pub_keys)?;
    let nftables_ruleset = firewall::render_nftables(manifest, peer_name)?;
    let node_cert_json = serde_json::to_string_pretty(&cert)?;
    let ca_cert_json = serde_json::to_string_pretty(&ca.ca_cert)?;

    Ok(PeerBundle {
        peer_name: peer_name.to_string(),
        interface: manifest.network.interface.clone(),
        wg_config,
        nftables_ruleset,
        node_cert_json,
        ca_cert_json,
    })
}

#[cfg(unix)]
fn write_owner_only(path: &Path, content: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    file.write_all(content)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn write_owner_only(path: &Path, content: &[u8]) -> Result<()> {
    std::fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Write a [`PeerBundle`] to a local directory with `0o600` permissions on all
/// files.
pub fn write_bundle_local(bundle: &PeerBundle, target_dir: &Path) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(target_dir)
        .with_context(|| format!("creating bundle directory {}", target_dir.display()))?;

    let wg_path = target_dir.join(format!("{}.conf", bundle.interface));
    let nft_path = target_dir.join(format!("{}.nft", bundle.interface));
    let cert_path = target_dir.join(format!("{}.cert", bundle.peer_name));
    let ca_path = target_dir.join("ca.cert");

    write_owner_only(&wg_path, bundle.wg_config.as_bytes())?;
    write_owner_only(&nft_path, bundle.nftables_ruleset.as_bytes())?;
    write_owner_only(&cert_path, bundle.node_cert_json.as_bytes())?;
    write_owner_only(&ca_path, bundle.ca_cert_json.as_bytes())?;

    Ok(vec![wg_path, nft_path, cert_path, ca_path])
}

fn validate_ssh_destination(dest: &str) -> Result<()> {
    let trimmed = dest.trim();
    if trimmed.is_empty() || trimmed.starts_with('-') {
        bail!("invalid SSH destination '{dest}'");
    }
    if trimmed
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '`' | '$' | '\'' | '"'))
    {
        bail!("invalid characters in SSH destination '{dest}'");
    }
    Ok(())
}

fn validate_remote_dir(remote_dir: &str) -> Result<()> {
    if remote_dir.is_empty() || remote_dir.starts_with('-') {
        bail!("invalid remote directory '{remote_dir}'");
    }
    if remote_dir
        .chars()
        .any(|c| c.is_control() || matches!(c, ';' | '&' | '|' | '`' | '$' | '\'' | '"'))
    {
        bail!("invalid characters in remote directory '{remote_dir}'");
    }
    Ok(())
}

/// Push a [`PeerBundle`] to a remote host using an injected `(destination, remote_filename, bytes)`
/// transport closure.
pub fn push_bundle_with<F>(
    bundle: &PeerBundle,
    destination: &str,
    remote_dir: &str,
    mut transport: F,
) -> Result<()>
where
    F: FnMut(&str, &str, &[u8]) -> Result<()>,
{
    validate_ssh_destination(destination)?;
    validate_remote_dir(remote_dir)?;

    let files = [
        (
            format!(
                "{}/{}.conf",
                remote_dir.trim_end_matches('/'),
                bundle.interface
            ),
            bundle.wg_config.as_bytes(),
        ),
        (
            format!(
                "{}/{}.nft",
                remote_dir.trim_end_matches('/'),
                bundle.interface
            ),
            bundle.nftables_ruleset.as_bytes(),
        ),
        (
            format!(
                "{}/{}.cert",
                remote_dir.trim_end_matches('/'),
                bundle.peer_name
            ),
            bundle.node_cert_json.as_bytes(),
        ),
        (
            format!("{}/ca.cert", remote_dir.trim_end_matches('/')),
            bundle.ca_cert_json.as_bytes(),
        ),
    ];

    for (remote_path, content) in files {
        transport(destination, &remote_path, content)
            .with_context(|| format!("pushing {remote_path} to {destination}"))?;
    }

    Ok(())
}

/// Push a [`PeerBundle`] to a remote node over SSH, streaming each file via
/// piped `stdin` under `umask 077` so private keys never appear in process
/// arguments or world-readable files.
pub fn push_bundle_ssh(bundle: &PeerBundle, destination: &str, remote_dir: &str) -> Result<()> {
    push_bundle_with(
        bundle,
        destination,
        remote_dir,
        |dest, remote_path, bytes| {
            let parent = Path::new(remote_path)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("/etc/aegis-net");
            let remote_cmd = format!("umask 077 && mkdir -p '{parent}' && cat > '{remote_path}'");

            let mut child = std::process::Command::new("ssh")
                .args(["-o", "ConnectTimeout=10", dest, &remote_cmd])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .context("spawning ssh")?;

            child
                .stdin
                .take()
                .ok_or_else(|| anyhow::anyhow!("ssh stdin was not piped"))?
                .write_all(bytes)
                .context("streaming bundle file over ssh stdin")?;

            let out = child.wait_with_output().context("waiting for ssh")?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                bail!("ssh to {dest} failed: {}", stderr.trim());
            }
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::{self, AddPeerSpec};
    use std::net::{IpAddr, Ipv4Addr};

    fn setup_fixture_network(dir: &Path) -> NetworkManifest {
        let ca = CertificateAuthority::generate("test-mesh", 365).unwrap();
        ca.save(&dir.join("ca")).unwrap();

        let toml_str = format!(
            r#"
[network]
name = "test-mesh"
cidr = "203.0.113.0/24"
config_dir = "{}"
interface = "aegis0"

[firewall]
default_action = "deny"

[[firewall.inbound]]
port = "443"
proto = "tcp"
groups = ["servers"]
action = "allow"
"#,
            dir.display()
        );
        let mut manifest: NetworkManifest = toml::from_str(&toml_str).unwrap();

        for (name, last_octet, lighthouse) in [("vps", 1, true), ("app", 2, false)] {
            let kp = WgKeypair::generate();
            let peer_dir = dir.join("peers").join(name);
            kp.save(&peer_dir, name).unwrap();
            peer::add_peer(
                &mut manifest,
                &ca,
                dir,
                AddPeerSpec {
                    name: name.to_string(),
                    ip: IpAddr::V4(Ipv4Addr::new(203, 0, 113, last_octet)),
                    groups: vec!["servers".to_string()],
                    endpoint: lighthouse.then(|| "198.51.100.1:51820".to_string()),
                    lighthouse,
                    cert_duration_days: 365,
                },
            )
            .unwrap();
        }

        manifest
    }

    #[test]
    #[cfg(unix)]
    fn build_and_write_bundle_local_verifies_cert_and_sets_0600() {
        use std::os::unix::fs::MetadataExt;

        let dir = tempfile::tempdir().unwrap();
        let manifest = setup_fixture_network(dir.path());

        let bundle = build_peer_bundle(&manifest, dir.path(), "app").unwrap();
        assert!(bundle.wg_config.contains("[Interface]"));
        assert!(bundle.wg_config.contains("203.0.113.2/32"));
        assert!(bundle.wg_config.contains("[Peer]"));
        assert!(bundle.nftables_ruleset.contains("table inet aegis_aegis0"));

        let out_dir = dir.path().join("bundle-out");
        let written = write_bundle_local(&bundle, &out_dir).unwrap();
        assert_eq!(written.len(), 4);

        for path in &written {
            let mode = std::fs::metadata(path).unwrap().mode() & 0o777;
            assert_eq!(
                mode,
                0o600,
                "expected 0600 on {}, got {:04o}",
                path.display(),
                mode
            );
        }
    }

    #[test]
    fn build_peer_bundle_rejects_tampered_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = setup_fixture_network(dir.path());

        let cert_path = dir.path().join("peers").join("app").join("app.cert");
        let mut cert = NodeCertificate::load(&cert_path).unwrap();
        cert.groups = vec!["admin".to_string()];
        std::fs::write(&cert_path, serde_json::to_string_pretty(&cert).unwrap()).unwrap();

        let err = build_peer_bundle(&manifest, dir.path(), "app").unwrap_err();
        assert!(
            format!("{err:#}").contains("verifying certificate"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn push_bundle_streams_all_four_files_and_rejects_bad_destinations() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = setup_fixture_network(dir.path());
        let bundle = build_peer_bundle(&manifest, dir.path(), "app").unwrap();

        let mut received: HashMap<String, Vec<u8>> = HashMap::new();
        push_bundle_with(
            &bundle,
            "root@203.0.113.2",
            "/etc/aegis-net",
            |dest, path, bytes| {
                assert_eq!(dest, "root@203.0.113.2");
                received.insert(path.to_string(), bytes.to_vec());
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(received.len(), 4);
        assert_eq!(
            received["/etc/aegis-net/aegis0.conf"],
            bundle.wg_config.as_bytes()
        );
        assert_eq!(
            received["/etc/aegis-net/aegis0.nft"],
            bundle.nftables_ruleset.as_bytes()
        );

        for bad_dest in ["", "-oProxyCommand=evil", "root@host;id"] {
            assert!(
                push_bundle_with(&bundle, bad_dest, "/etc/aegis-net", |_, _, _| Ok(())).is_err(),
                "expected rejection for destination {bad_dest:?}"
            );
        }
    }
}
