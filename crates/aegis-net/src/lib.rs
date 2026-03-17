//! aegis-net: DIRMACS overlay network
//!
//! WireGuard-based encrypted mesh network with declarative TOML configuration,
//! certificate-based authentication, and security group firewall rules.
//!
//! Inspired by Slack's Nebula but built for the dirmacs ecosystem:
//! - Own CA with Ed25519 certificates
//! - Peers identified by name + groups (like Nebula security groups)
//! - Firewall rules reference groups, not IPs
//! - Declarative TOML config → WireGuard interface configs
//! - Managed through `aegis net` CLI

pub mod ca;
pub mod config;
pub mod firewall;
pub mod peer;
pub mod status;
pub mod wg;
