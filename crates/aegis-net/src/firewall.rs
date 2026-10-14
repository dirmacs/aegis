//! Security group firewall rule evaluation.
//!
//! Rules reference groups (not IPs) — like Nebula's security groups.
//! When generating WireGuard configs, rules are resolved to AllowedIPs.

use crate::config::{FirewallAction, FirewallConfig, FirewallRule, NetworkManifest};

/// Evaluate whether a connection should be allowed.
pub fn evaluate(
    config: &FirewallConfig,
    direction: Direction,
    peer_groups: &[String],
    peer_name: &str,
    port: u16,
    proto: &str,
) -> FirewallAction {
    let rules = match direction {
        Direction::Inbound => &config.inbound,
        Direction::Outbound => &config.outbound,
    };

    for rule in rules {
        if matches_rule(rule, peer_groups, peer_name, port, proto) {
            return rule.action;
        }
    }

    config.default_action
}

/// Traffic direction.
#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Inbound,
    Outbound,
}

fn matches_rule(
    rule: &FirewallRule,
    peer_groups: &[String],
    peer_name: &str,
    port: u16,
    proto: &str,
) -> bool {
    // Check port match
    if !matches_port(&rule.port, port) {
        return false;
    }

    // Check protocol match
    if rule.proto != "any" && rule.proto != proto {
        return false;
    }

    // If no groups or peers specified, matches everything
    if rule.groups.is_empty() && rule.peers.is_empty() {
        return true;
    }

    // Check group match
    if rule.groups.iter().any(|g| peer_groups.contains(g)) {
        return true;
    }

    // Check peer name match
    if rule.peers.iter().any(|p| p == peer_name) {
        return true;
    }

    false
}

fn matches_port(rule_port: &str, actual_port: u16) -> bool {
    if rule_port == "any" {
        return true;
    }

    // Single port
    if let Ok(p) = rule_port.parse::<u16>() {
        return p == actual_port;
    }

    // Port range (e.g., "8000-9000")
    if let Some((start, end)) = rule_port.split_once('-')
        && let (Ok(s), Ok(e)) = (start.parse::<u16>(), end.parse::<u16>())
    {
        return actual_port >= s && actual_port <= e;
    }

    false
}

/// Resolve firewall rules to a set of allowed peer IPs for WireGuard config generation.
pub fn resolve_allowed_peers(manifest: &NetworkManifest, for_peer: &str) -> Vec<String> {
    let _peer = match manifest.peer(for_peer) {
        Some(p) => p,
        None => return vec![],
    };

    let mut allowed = Vec::new();

    for (name, other_peer) in &manifest.peers {
        if name == for_peer {
            continue;
        }

        // Check if any inbound rule allows traffic from this peer
        let other_groups = &other_peer.groups;
        let should_allow = manifest.firewall.inbound.iter().any(|rule| {
            if rule.action != FirewallAction::Allow {
                return false;
            }
            // If rule has no groups/peers filter, it allows everything
            if rule.groups.is_empty() && rule.peers.is_empty() {
                return true;
            }
            // Check group match
            if rule.groups.iter().any(|g| other_groups.contains(g)) {
                return true;
            }
            // Check peer name match
            rule.peers.iter().any(|p| p == name.as_str())
        });

        if should_allow {
            allowed.push(format!("{}/32", other_peer.ip));
        }
    }

    // Lighthouses always get full mesh access
    for (name, other_peer) in &manifest.peers {
        if name == for_peer {
            continue;
        }
        if other_peer.lighthouse {
            let entry = format!("{}/32", other_peer.ip);
            if !allowed.contains(&entry) {
                allowed.push(entry);
            }
        }
    }

    allowed
}

fn validate_interface_name(iface: &str) -> anyhow::Result<()> {
    if iface.is_empty() || iface.len() > 15 {
        anyhow::bail!("invalid interface name '{iface}': must be 1..=15 ASCII characters");
    }
    if !iface
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        anyhow::bail!(
            "invalid interface name '{iface}': only ASCII alphanumeric, '_', '-', '.' allowed"
        );
    }
    Ok(())
}

fn format_nft_port_clause(proto: &str, port: &str) -> anyhow::Result<String> {
    if port == "any" {
        return match proto {
            "any" => Ok(String::new()),
            "tcp" | "udp" => Ok(format!("meta l4proto {proto} ")),
            other => anyhow::bail!("unsupported firewall protocol '{other}'"),
        };
    }

    let validated_port = if let Ok(p) = port.parse::<u16>() {
        p.to_string()
    } else if let Some((s, e)) = port.split_once('-') {
        let start: u16 = s
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid port range '{port}'"))?;
        let end: u16 = e
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid port range '{port}'"))?;
        if start > end {
            anyhow::bail!("invalid port range '{port}': start > end");
        }
        format!("{start}-{end}")
    } else {
        anyhow::bail!("invalid port specification '{port}'");
    };

    match proto {
        "tcp" | "udp" => Ok(format!("{proto} dport {validated_port} ")),
        "any" => Ok(format!(
            "meta l4proto {{ tcp, udp }} th dport {validated_port} "
        )),
        other => anyhow::bail!("unsupported firewall protocol '{other}'"),
    }
}

/// Render an atomic `nftables` ruleset enforcing the manifest's inbound and
/// outbound security-group rules for `for_peer` on the overlay interface.
///
/// WireGuard `AllowedIPs` only filters at the IP layer (`/32`); this ruleset
/// enforces the per-port, per-protocol, and per-group policy on the overlay
/// interface (`iifname` / `oifname`) without touching non-overlay host traffic.
pub fn render_nftables(manifest: &NetworkManifest, for_peer: &str) -> anyhow::Result<String> {
    if manifest.peer(for_peer).is_none() {
        anyhow::bail!("peer '{for_peer}' not found in manifest");
    }

    let iface = &manifest.network.interface;
    validate_interface_name(iface)?;
    let table_suffix: String = iface
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let table_name = format!("aegis_{table_suffix}");

    let mut peer_names: Vec<&String> = manifest.peers.keys().collect();
    peer_names.sort();

    let mut out = String::new();
    out.push_str(&format!("table inet {table_name}\n"));
    out.push_str(&format!("delete table inet {table_name}\n"));
    out.push_str(&format!("table inet {table_name} {{\n"));

    // Inbound chain scoped strictly to the overlay interface
    out.push_str("    chain input {\n");
    out.push_str("        type filter hook input priority 0; policy accept;\n");
    out.push_str(&format!("        iifname != \"{iface}\" accept\n"));
    out.push_str("        ct state established,related accept\n");

    for rule in &manifest.firewall.inbound {
        let port_clause = format_nft_port_clause(&rule.proto, &rule.port)?;
        let verdict = match rule.action {
            FirewallAction::Allow => "accept",
            FirewallAction::Deny => "drop",
        };

        if rule.groups.is_empty() && rule.peers.is_empty() {
            out.push_str(&format!("        {port_clause}{verdict}\n"));
            continue;
        }

        for name in &peer_names {
            if name.as_str() == for_peer {
                continue;
            }
            let other = &manifest.peers[*name];
            let group_match = rule.groups.iter().any(|g| other.groups.contains(g));
            let name_match = rule.peers.iter().any(|p| p == *name);
            if group_match || name_match {
                out.push_str(&format!(
                    "        ip saddr {} {port_clause}{verdict}\n",
                    other.ip
                ));
            }
        }
    }

    let default_verdict = match manifest.firewall.default_action {
        FirewallAction::Allow => "accept",
        FirewallAction::Deny => "drop",
    };
    out.push_str(&format!("        iifname \"{iface}\" {default_verdict}\n"));
    out.push_str("    }\n");

    // Outbound chain scoped strictly to the overlay interface (if outbound rules exist)
    if !manifest.firewall.outbound.is_empty() {
        out.push_str("    chain output {\n");
        out.push_str("        type filter hook output priority 0; policy accept;\n");
        out.push_str(&format!("        oifname != \"{iface}\" accept\n"));
        out.push_str("        ct state established,related accept\n");

        for rule in &manifest.firewall.outbound {
            let port_clause = format_nft_port_clause(&rule.proto, &rule.port)?;
            let verdict = match rule.action {
                FirewallAction::Allow => "accept",
                FirewallAction::Deny => "drop",
            };

            if rule.groups.is_empty() && rule.peers.is_empty() {
                out.push_str(&format!("        {port_clause}{verdict}\n"));
                continue;
            }

            for name in &peer_names {
                if name.as_str() == for_peer {
                    continue;
                }
                let other = &manifest.peers[*name];
                let group_match = rule.groups.iter().any(|g| other.groups.contains(g));
                let name_match = rule.peers.iter().any(|p| p == *name);
                if group_match || name_match {
                    out.push_str(&format!(
                        "        ip daddr {} {port_clause}{verdict}\n",
                        other.ip
                    ));
                }
            }
        }

        out.push_str("    }\n");
    }

    out.push_str("}\n");
    Ok(out)
}

/// Apply a rendered `nftables` ruleset by piping it into `nft -f -` via the
/// supplied command builder.
pub fn apply_nftables_with<F>(ruleset: &str, make_cmd: F) -> anyhow::Result<()>
where
    F: FnOnce() -> std::process::Command,
{
    use anyhow::Context;
    use std::io::Write;

    if ruleset.trim().is_empty() {
        anyhow::bail!("refusing to apply empty nftables ruleset");
    }

    let mut cmd = make_cmd();
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("spawning nft")?;

    child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("nft stdin was not piped"))?
        .write_all(ruleset.as_bytes())
        .context("writing nftables ruleset to stdin")?;

    let output = child.wait_with_output().context("waiting for nft")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("nft -f - failed: {}", stderr.trim());
    }

    Ok(())
}

/// Apply a rendered `nftables` ruleset to the host via `nft -f -`.
pub fn apply_nftables(ruleset: &str) -> anyhow::Result<()> {
    apply_nftables_with(ruleset, || {
        let mut cmd = std::process::Command::new("nft");
        cmd.args(["-f", "-"]);
        cmd
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FirewallAction;

    fn test_config() -> FirewallConfig {
        FirewallConfig {
            default_action: FirewallAction::Deny,
            inbound: vec![
                FirewallRule {
                    port: "any".to_string(),
                    proto: "any".to_string(),
                    groups: vec!["admin".to_string()],
                    peers: vec![],
                    action: FirewallAction::Allow,
                },
                FirewallRule {
                    port: "443".to_string(),
                    proto: "tcp".to_string(),
                    groups: vec!["servers".to_string()],
                    peers: vec![],
                    action: FirewallAction::Allow,
                },
                FirewallRule {
                    port: "8000-9000".to_string(),
                    proto: "tcp".to_string(),
                    groups: vec![],
                    peers: vec!["monitoring".to_string()],
                    action: FirewallAction::Allow,
                },
            ],
            outbound: vec![],
        }
    }

    #[test]
    fn admin_group_allows_any() {
        let config = test_config();
        let result = evaluate(
            &config,
            Direction::Inbound,
            &["admin".to_string()],
            "alice",
            22,
            "tcp",
        );
        assert_eq!(result, FirewallAction::Allow);
    }

    #[test]
    fn server_group_allows_443_only() {
        let config = test_config();
        assert_eq!(
            evaluate(
                &config,
                Direction::Inbound,
                &["servers".to_string()],
                "web",
                443,
                "tcp"
            ),
            FirewallAction::Allow
        );
        assert_eq!(
            evaluate(
                &config,
                Direction::Inbound,
                &["servers".to_string()],
                "web",
                22,
                "tcp"
            ),
            FirewallAction::Deny
        );
    }

    #[test]
    fn port_range_match() {
        let config = test_config();
        assert_eq!(
            evaluate(&config, Direction::Inbound, &[], "monitoring", 8081, "tcp"),
            FirewallAction::Allow
        );
        assert_eq!(
            evaluate(&config, Direction::Inbound, &[], "monitoring", 7999, "tcp"),
            FirewallAction::Deny
        );
    }

    #[test]
    fn unknown_group_denied() {
        let config = test_config();
        assert_eq!(
            evaluate(
                &config,
                Direction::Inbound,
                &["unknown".to_string()],
                "rando",
                80,
                "tcp"
            ),
            FirewallAction::Deny
        );
    }

    #[test]
    fn render_nftables_scopes_to_interface_and_translates_group_rules() {
        let toml_str = r#"
[network]
name = "mesh"
cidr = "10.42.0.0/24"
interface = "aegis0"

[peers.vps]
ip = "10.42.0.1"
groups = ["servers"]

[peers.admin_box]
ip = "10.42.0.2"
groups = ["admin"]

[peers.monitoring]
ip = "10.42.0.3"
groups = ["ops"]

[firewall]
default_action = "deny"

[[firewall.inbound]]
port = "any"
proto = "any"
groups = ["admin"]
action = "allow"

[[firewall.inbound]]
port = "443"
proto = "tcp"
groups = ["servers"]
action = "allow"

[[firewall.inbound]]
port = "8000-9000"
proto = "tcp"
peers = ["monitoring"]
action = "allow"
"#;
        let manifest: NetworkManifest = toml::from_str(toml_str).unwrap();
        let ruleset = render_nftables(&manifest, "vps").unwrap();

        assert!(ruleset.contains("table inet aegis_aegis0"));
        assert!(ruleset.contains("iifname != \"aegis0\" accept"));
        assert!(ruleset.contains("ip saddr 10.42.0.2 accept"));
        assert!(ruleset.contains("ip saddr 10.42.0.3 tcp dport 8000-9000 accept"));
        // Self ("vps" = 10.42.0.1) must not be emitted as an inbound source rule on itself
        assert!(!ruleset.contains("ip saddr 10.42.0.1"));
        assert!(ruleset.contains("iifname \"aegis0\" drop"));
    }

    #[test]
    fn render_nftables_rejects_unsafe_interface_name() {
        let toml_str = r#"
[network]
name = "mesh"
cidr = "10.42.0.0/24"
interface = "aegis0\"; flush ruleset; #"

[peers.vps]
ip = "10.42.0.1"
"#;
        let manifest: NetworkManifest = toml::from_str(toml_str).unwrap();
        assert!(render_nftables(&manifest, "vps").is_err());
    }

    #[test]
    fn apply_nftables_delivers_ruleset_over_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let captured = dir.path().join("captured.nft");
        let ruleset = "table inet aegis_aegis0 {\n}\n";

        apply_nftables_with(ruleset, || {
            let mut cmd = std::process::Command::new("sh");
            cmd.arg("-c").arg(format!("cat > {}", captured.display()));
            cmd
        })
        .unwrap();

        let disk = std::fs::read_to_string(&captured).unwrap();
        assert_eq!(disk, ruleset);
    }
}
