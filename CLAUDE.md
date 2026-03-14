# Aegis

Dirmacs system configuration manager. 4-crate Rust workspace. Manages dotfiles, configs, OpenCode generation, and dirmacs toolchain via declarative TOML manifests.

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
| `aegis-opencode` | Typed TOML → opencode.json + oh-my-opencode.json |
| `aegis-toolchain` | Dirmacs ecosystem install, update, health checks |
| `aegis-cli` | Clap-based CLI binary |

## Config Paths

- `aegis.toml` — manifest in cwd or `~/.config/aegis/aegis.toml`
- `example/aegis.toml` — reference manifest with all fields
- Templates use Tera syntax: `{{ variable }}`, `{% if profile == "dev" %}`

## Key Rules

- **Manifest schema is defined in `aegis-core/src/manifest.rs`** — add fields there first
- **`aegis-opencode` reads from `lancor` crate** for LLM model definitions — check lancor API before changing model types
- **Profile names are free-form strings** — no hardcoded profile list; `dev-vps`, `workstation`, `ci` are just conventions
- **`aegis toolchain` subcommands** call out to system to install tools — test with `--dry-run` first

## Git Author

Always commit as bkataru:
```bash
git -c user.name="bkataru" -c user.email="baalateja.k@gmail.com" commit
```
