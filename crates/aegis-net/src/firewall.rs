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
    if let Some((start, end)) = rule_port.split_once('-') {
        if let (Ok(s), Ok(e)) = (start.parse::<u16>(), end.parse::<u16>()) {
            return actual_port >= s && actual_port <= e;
        }
    }

    false
}

/// Resolve firewall rules to a set of allowed peer IPs for WireGuard config generation.
pub fn resolve_allowed_peers(
    manifest: &NetworkManifest,
    for_peer: &str,
) -> Vec<String> {
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
            "baala",
            22,
            "tcp",
        );
        assert_eq!(result, FirewallAction::Allow);
    }

    #[test]
    fn server_group_allows_443_only() {
        let config = test_config();
        assert_eq!(
            evaluate(&config, Direction::Inbound, &["servers".to_string()], "web", 443, "tcp"),
            FirewallAction::Allow
        );
        assert_eq!(
            evaluate(&config, Direction::Inbound, &["servers".to_string()], "web", 22, "tcp"),
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
            evaluate(&config, Direction::Inbound, &["unknown".to_string()], "rando", 80, "tcp"),
            FirewallAction::Deny
        );
    }
}
