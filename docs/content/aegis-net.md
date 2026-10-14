# aegis-net: Overlay Network

`aegis-net` is a WireGuard-based encrypted mesh networking crate built into the Aegis workspace. It provides declarative TOML-driven overlay networking with Nebula-inspired security groups, `nftables` host firewall enforcement, and verified peer bundle distribution.

## Architecture

| Module | Purpose |
|--------|---------|
| `ca.rs` | Ed25519 certificate authority — CA generation, node cert signing/verification, HTTP request proof |
| `config.rs` | `NetworkManifest` TOML types — peers, groups, firewall rules, lighthouse config |
| `firewall.rs` | Security group rule evaluation and atomic `nftables` ruleset rendering/application |
| `peer.rs` | Peer management — add/remove/list, auto IP assignment within CIDR |
| `wg.rs` | x25519 WireGuard keypair generation, `wg-quick` config output, and `up`/`down` lifecycle |
| `distribute.rs` | CA-verified peer bundle packaging (`<iface>.conf`, `<iface>.nft`, `<peer>.cert`, `ca.cert`) and SSH push |
| `status.rs` | Network health checks — CA validity, cert expiry, interface status |

## Quick Start

```bash
# Initialize network with CA
aegis net init --name example-mesh --cidr 10.42.0.0/24

# Add peers (using RFC 5737 documentation IP for the lighthouse endpoint)
aegis net add-peer lighthouse --ip 10.42.0.1 --groups servers --lighthouse --endpoint 203.0.113.1:51820
aegis net add-peer alice-laptop --groups admin,dev
aegis net add-peer bob-workstation --groups admin,dev

# Generate WireGuard configs
aegis net generate

# Bring up the overlay interface and apply nftables firewall rules
sudo aegis net up --peer lighthouse --apply-firewall

# Package or distribute a verified peer bundle over SSH
sudo aegis net distribute app-server --to admin@203.0.113.10
```

## Network Manifest (`aegis-net.toml`)

See `example/aegis-net.toml` for a complete reference manifest. By default, `aegis net` reads `./aegis-net.toml` if present, or falls back to `/etc/aegis-net/aegis-net.toml`.

```toml
[network]
name = "example-mesh"
cidr = "10.42.0.0/24"
config_dir = "/etc/aegis-net"
listen_port = 51820
mtu = 1300
interface = "aegis0"

[peers.lighthouse]
ip = "10.42.0.1"
groups = ["servers"]
endpoint = "203.0.113.1:51820"
lighthouse = true

[peers.alice-laptop]
ip = "10.42.0.2"
groups = ["admin", "dev"]

[[firewall.inbound]]
port = "any"
groups = ["admin"]
action = "allow"

[[firewall.inbound]]
port = "3000-9000"
proto = "tcp"
groups = ["servers"]
action = "allow"
```

## Security Model

- **CA**: Ed25519; certificates embed node IP, name, and groups, signed by the CA.
- **WireGuard**: x25519 Curve25519 key exchange, per-peer configs verified to mode `0600` before `wg-quick up`.
- **Firewall**: Rules reference groups (like Nebula security groups), resolved both to WireGuard `AllowedIPs` and to an atomic `nftables` `table inet aegis_<iface>` ruleset scoped strictly to the overlay interface.
- **Key storage**: All private keys, vaults, and peer bundles are created with `O_CREAT` mode `0600`.
