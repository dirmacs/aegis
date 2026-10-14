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

/// Normalize an expected AllowedIP spelling into a canonical CIDR form.
///
/// Bare IP addresses (as `wg` sometimes reports for a peer) are treated as
/// host routes, i.e. `/32` for IPv4 and `/128` for IPv6.
pub fn normalize_allowed_ip(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value.contains('/') {
        Some(value.to_string())
    } else if value.contains(':') {
        Some(format!("{value}/128"))
    } else {
        Some(format!("{value}/32"))
    }
}

/// Inspect one line of `ip -o route show dev <iface>` output and return the
/// destination it routes, but only when that route actually uses `iface`
/// directly (no gateway, no other device).
///
/// A `via` token always disqualifies the route: that is the kernel forwarding
/// the packet through a gateway rather than into the mesh, which is exactly the
/// failure mode this guard exists to catch.
fn route_destination_via_iface(iface: &str, line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let destination = tokens[0];
    if destination.eq_ignore_ascii_case("default") {
        // A default route is never a mesh peer route.
        return None;
    }

    let mut has_via = false;
    let mut dev: Option<&str> = None;
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "via" => has_via = true,
            "dev" => {
                if let Some(next) = tokens.get(i + 1) {
                    dev = Some(*next);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    if has_via {
        return None;
    }
    if let Some(device) = dev
        && device != iface
    {
        return None;
    }

    normalize_allowed_ip(destination)
}

/// Decide which of the expected mesh AllowedIPs are not actually routed via
/// `iface` according to the supplied route-table text.
///
/// An empty or whitespace-only table is treated as "everything missing": a
/// table that could not be read must never be reported as a clean mesh.
pub fn missing_allowed_ip_routes<T: AsRef<str>>(
    iface: &str,
    expected_allowed_ips: &[T],
    route_table: &str,
) -> Vec<String> {
    if route_table.trim().is_empty() {
        return expected_allowed_ips
            .iter()
            .filter_map(|value| normalize_allowed_ip(value.as_ref()))
            .collect();
    }

    let routed: std::collections::HashSet<String> = route_table
        .lines()
        .filter_map(|line| route_destination_via_iface(iface, line))
        .collect();

    let mut missing = Vec::new();
    for value in expected_allowed_ips {
        if let Some(normalized) = normalize_allowed_ip(value.as_ref())
            && !routed.contains(&normalized)
            && !missing.contains(&normalized)
        {
            missing.push(normalized);
        }
    }

    // A table that parsed no usable routes at all cannot be considered healthy.
    // This keeps a truncated/garbage table from turning into a vacuous pass.
    if routed.is_empty() && !expected_allowed_ips.is_empty() {
        for value in expected_allowed_ips {
            if let Some(normalized) = normalize_allowed_ip(value.as_ref())
                && !missing.contains(&normalized)
            {
                missing.push(normalized);
            }
        }
    }

    missing
}

/// Pure predicate: every expected AllowedIP is routed directly through
/// `iface` according to `route_table`.
pub fn allowed_ips_routed<T: AsRef<str>>(
    iface: &str,
    expected_allowed_ips: &[T],
    route_table: &str,
) -> bool {
    missing_allowed_ip_routes(iface, expected_allowed_ips, route_table).is_empty()
        && (!route_table.trim().is_empty() || expected_allowed_ips.is_empty())
}

/// Fail-closed verification used by `aegis net up`: returns an error naming the
/// missing AllowedIPs (or an unreadable route table).
pub fn verify_allowed_ip_routes<T: AsRef<str>>(
    iface: &str,
    expected_allowed_ips: &[T],
    route_table: &str,
) -> Result<()> {
    if route_table.trim().is_empty() {
        anyhow::bail!(
            "could not verify mesh routes for {iface}: route table is empty or unreadable"
        );
    }
    let missing = missing_allowed_ip_routes(iface, expected_allowed_ips, route_table);
    if !missing.is_empty() {
        anyhow::bail!(
            "mesh routes missing on {iface}; AllowedIPs not routed via {iface}: {}",
            missing.join(", ")
        );
    }
    Ok(())
}

/// Read the kernel route table for `iface`.
///
/// This shells out to `ip -o route show dev <iface>` and fails closed if the
/// command cannot be spawned or exits non-zero.
pub fn read_route_table(iface: &str) -> Result<String> {
    let output = std::process::Command::new("ip")
        .args(["-o", "route", "show", "dev", iface])
        .output()
        .with_context(|| format!("spawning ip route show dev {iface}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ip route show dev {iface} failed: {}", stderr.trim());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.trim().is_empty() {
        anyhow::bail!("ip route show dev {iface} returned no routes");
    }
    Ok(stdout)
}

/// Read the AllowedIPs that WireGuard currently has configured for `iface`.
pub fn allowed_ips_for_interface(iface: &str) -> Result<Vec<String>> {
    let output = std::process::Command::new("wg")
        .args(["show", iface, "allowed-ips"])
        .output()
        .with_context(|| format!("spawning wg show {iface} allowed-ips"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("wg show {iface} allowed-ips failed: {}", stderr.trim());
    }

    let mut allowed = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        for token in line.split_whitespace() {
            for item in token.split(',') {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                // Skip base64 public-key tokens.
                if (item.contains('/')
                    || (item.contains('.') && !item.contains('='))
                    || item.contains(':'))
                    && let Some(normalized) = normalize_allowed_ip(item)
                {
                    // Avoid recording the same CIDR twice.
                    if !allowed.contains(&normalized) {
                        allowed.push(normalized);
                    }
                }
            }
        }
    }

    Ok(allowed)
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
cidr = "203.0.113.0/24"

[peers.vps]
ip = "203.0.113.1"
groups = ["servers"]
lighthouse = true
endpoint = "1.2.3.4:51820"

[peers.laptop]
ip = "203.0.113.2"
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
        assert!(config.contains("203.0.113.2/32"));
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

    #[test]
    fn route_guard_passes_wg_quick_style_routes() {
        let iface = "aegis0";
        let expected = ["203.0.113.2/32", "203.0.113.3/32"];
        let table = "203.0.113.2/32 dev aegis0\n203.0.113.3/32 dev aegis0 metric 100\n";
        assert!(allowed_ips_routed(iface, &expected, table));
        assert!(missing_allowed_ip_routes(iface, &expected, table).is_empty());
    }

    #[test]
    fn route_guard_fails_syncconf_only() {
        let iface = "aegis0";
        let expected = ["203.0.113.2/32", "203.0.113.3/32"];
        let table = "";
        assert!(!allowed_ips_routed(iface, &expected, table));
        let missing = missing_allowed_ip_routes(iface, &expected, table);
        assert_eq!(
            missing,
            vec!["203.0.113.2/32".to_string(), "203.0.113.3/32".to_string()]
        );
    }

    #[test]
    fn route_guard_fails_route_via_other_device_or_gateway() {
        let iface = "aegis0";
        let expected = ["203.0.113.2/32"];
        for table in [
            "203.0.113.2/32 dev eth0",
            "203.0.113.2/32 via 192.168.1.1 dev eth0",
            "203.0.113.2 via 192.0.2.1 dev aegis0",
        ] {
            assert!(
                !allowed_ips_routed(iface, &expected, table),
                "table was {table:?}"
            );
            assert_eq!(
                missing_allowed_ip_routes(iface, &expected, table),
                vec!["203.0.113.2/32".to_string()]
            );
        }
    }

    #[test]
    fn route_guard_handles_spelled_route_lines() {
        let iface = "aegis0";
        let expected = ["203.0.113.12/32"];
        assert!(allowed_ips_routed(
            iface,
            &expected,
            "203.0.113.12 scope link\n"
        ));
        assert!(allowed_ips_routed(
            iface,
            &expected,
            "203.0.113.12/32 dev aegis0\n"
        ));
        assert!(allowed_ips_routed(
            iface,
            &expected,
            "203.0.113.12 dev aegis0 metric 100\n"
        ));
    }

    #[test]
    fn route_guard_empty_table_fails() {
        let iface = "aegis0";
        let expected = ["203.0.113.2/32"];
        assert!(!allowed_ips_routed(iface, &expected, ""));
        assert!(!allowed_ips_routed(iface, &expected, "   \n\t  "));
    }

    #[test]
    fn route_guard_unreadable_table_fails() {
        let iface = "aegis0";
        let expected = ["203.0.113.2/32"];
        // A table that parses no usable routes must not look healthy.
        assert!(!allowed_ips_routed(iface, &expected, "not a route table"));
        assert_eq!(
            verify_allowed_ip_routes(iface, &expected, "not a route table")
                .unwrap_err()
                .to_string(),
            "mesh routes missing on aegis0; AllowedIPs not routed via aegis0: 203.0.113.2/32"
        );
    }

    #[test]
    fn route_guard_does_not_require_interface_local_32() {
        // The interface's own address is not in the expected AllowedIPs list
        // produced by `wg show allowed-ips`, so a route table containing only
        // peer routes still passes.
        let iface = "aegis0";
        let expected: [&str; 0] = [];
        let table = "203.0.113.1/32 dev aegis0\n";
        assert!(allowed_ips_routed(iface, &expected, table));
    }

    #[test]
    fn route_guard_error_names_missing_allowed_ips() {
        let iface = "aegis0";
        let expected = ["203.0.113.2/32", "203.0.113.9/32"];
        let table = "203.0.113.2/32 dev aegis0\n";
        let err = verify_allowed_ip_routes(iface, &expected, table).unwrap_err();
        let text = format!("{err:#}");
        assert!(
            text.contains("203.0.113.9/32"),
            "error did not name missing IP: {text}"
        );
        assert!(!text.contains("203.0.113.2/32 is missing".to_string().as_str()));
    }

    #[test]
    fn route_guard_verify_fails_closed_on_empty_table() {
        let iface = "aegis0";
        let expected = ["203.0.113.2/32"];
        let err = verify_allowed_ip_routes(iface, &expected, "").unwrap_err();
        assert!(format!("{err:#}").contains("empty or unreadable"));
    }
}
