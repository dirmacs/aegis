# aegis-net: DIRMACS Overlay Network

aegis-net is a WireGuard-based encrypted mesh networking crate built into the aegis workspace. It provides declarative TOML-driven overlay networking with Nebula-inspired security groups.

## Architecture

| Module | Purpose |
|--------|---------|
| `ca.rs` | Ed25519 certificate authority — CA generation, node cert signing/verification |
| `config.rs` | `NetworkManifest` TOML types — peers, groups, firewall rules, lighthouse config |
| `firewall.rs` | Security group rule evaluation — rules reference groups not IPs |
| `peer.rs` | Peer management — add/remove/list, auto IP assignment within CIDR |
| `wg.rs` | x25519 WireGuard keypair generation, wg-quick config output |
| `status.rs` | Network health checks — CA validity, cert expiry, interface status |

## Quick Start

```bash
# Initialize network with CA
aegis net init --name dirmacs-mesh --cidr 10.42.0.0/24

# Add peers
aegis net add-peer vps --ip 10.42.0.1 --groups servers --lighthouse --endpoint 217.216.78.38:51820
aegis net add-peer baala-laptop --groups admin,dev
aegis net add-peer shanjeth-mac --groups admin,dev

# Generate WireGuard configs
aegis net generate

# Deploy VPS config
sudo cp /etc/aegis-net/wg-configs/aegis0-vps.conf /etc/wireguard/aegis0.conf
sudo wg-quick up aegis0
sudo systemctl enable wg-quick@aegis0
```

## Network Manifest (aegis-net.toml)

```toml
[network]
name = "dirmacs-mesh"
cidr = "10.42.0.0/24"
config_dir = "/etc/aegis-net"
listen_port = 51820
mtu = 1300
interface = "aegis0"

[peers.vps]
ip = "10.42.0.1"
groups = ["servers"]
endpoint = "217.216.78.38:51820"
lighthouse = true

[peers.baala-laptop]
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

- **CA**: Ed25519, certs embed node IP + name + groups, signed by CA
- **WireGuard**: x25519 Curve25519 key exchange, per-peer configs
- **Firewall**: Rules reference groups (like Nebula security groups), resolved to AllowedIPs at config generation time
- **Key storage**: All keys chmod 600, CA key never leaves /etc/aegis-net/ca/

## Production Status

Deployed on DIRMACS VPS (217.216.78.38) since 2026-03-17:
- Interface `aegis0` UP at 10.42.0.1
- CA expires 2027-03-17
- 3 peers configured, WireGuard configs generated
- UFW 51820/udp open, systemd enabled on boot

## Tests

16 tests covering CA generation, cert signing/verification, firewall rule evaluation, WireGuard config generation, peer management, and key save/load roundtrips.
