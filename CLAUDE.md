# Aegis

Dirmacs system configuration manager. 6-crate Rust workspace. Manages dotfiles, configs, secrets, overlay networking, and dirmacs toolchain via declarative TOML manifests.

## Build & Test

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check

# Run CLI from workspace root
cargo run -p aegis-cli -- status
cargo run -p aegis-cli -- link
```

## Architecture

| Crate | Purpose |
|-------|---------|
| `aegis-core` | Manifest parsing, module system, Tera templates, diffing |
| `aegis-net` | Overlay network management (CA, peers, WireGuard, nftables) |
| `aegis-secrets` | Encrypted secrets vault (AES-256-GCM + Argon2id) |
| `aegis-serve` | Authenticated HTTP secrets keychain service |
| `aegis-toolchain` | Dirmacs ecosystem install, update, health checks |
| `aegis-cli` | Clap-based CLI binary |

## Config Paths

- `aegis.toml` — manifest in cwd or `~/.config/aegis/aegis.toml`
- `example/aegis.toml` — reference manifest with all fields
- Templates use Tera syntax: `{{ variable }}`, `{% if profile == "dev" %}`

## Key Rules

- **Manifest schema is defined in `aegis-core/src/manifest.rs`** — add fields there first
- **Profile names are free-form strings** — no hardcoded profile list; `dev-vps`, `workstation`, `ci` are just conventions
- **`aegis toolchain` subcommands** call out to system to install tools — test with `--dry-run` first

## Git Author

Always commit as bkataru:
```bash
git -c user.name="bkataru" -c user.email="baalateja.k@gmail.com" commit
```
