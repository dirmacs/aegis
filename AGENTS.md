# Aegis — Agent Context

Aegis is a Rust CLI tool that manages system configurations from declarative TOML manifests. Replaces shell-script dotfile managers with typed, profile-aware config management for the dirmacs ecosystem.

## Architecture

```
aegis-core/
  src/
    manifest.rs   — Top-level AegisManifest struct (root of all parsing)
    module.rs     — Module types: symlink, copy, template
    profile.rs    — Profile selection and variable scoping
    template.rs   — Tera template rendering for config files
    variables.rs  — Variable substitution and inheritance
    diff.rs       — Drift detection between target and source
    config.rs     — Config file search path resolution
    package.rs    — Package/dependency declarations

aegis-opencode/
  src/            — Generates opencode.json + oh-my-opencode.json from TOML

aegis-toolchain/
  src/            — Install/update/health-check ares, daedra, thulp, eruka, lancor

aegis-cli/
  src/            — Clap CLI: init, bootstrap, status, link, diff, opencode, toolchain
```

## Manifest Structure

```toml
[aegis]
name = "my-configs"
version = "1"

[variables]
editor = "hx"
theme = "gruvbox"

[profiles.dev-vps]
variables = { theme = "dark" }

[profiles.workstation]
variables = { editor = "zed" }

[[modules]]
name = "helix"
source = "helix/config.toml"
target = "~/.config/helix/config.toml"
method = "symlink"   # or "copy" or "template"
profiles = ["dev-vps", "workstation"]

[[modules]]
name = "opencode"
source = "opencode/opencode.toml"
target = "~/.config/opencode/opencode.toml"
method = "template"
```

## Common Tasks

**Add a new config module:**
1. Create source file in your config repo
2. Add `[[modules]]` entry to `aegis.toml`
3. Run `aegis link --module <name>` to deploy
4. Run `aegis diff --module <name>` to verify

**Add a new toolchain tool:**
1. Add struct implementing `Installable` trait in `aegis-toolchain/src/`
2. Register in `toolchain/mod.rs` dispatch
3. Test with `aegis toolchain install <tool> --dry-run`

**Generate OpenCode configs:**
```bash
aegis opencode generate    # writes to ~/.config/opencode/opencode.json
aegis opencode validate    # validates TOML model definitions
```

**Apply a specific profile:**
```bash
aegis link --profile workstation
aegis bootstrap --profile ci
```

## Key Decisions

- **Tera for templates** — not Handlebars/Jinja2; Tera is Rust-native and well-maintained
- **Symlink preferred over copy** — live edits to source are reflected immediately
- **`lancor` for model definitions** — aegis-opencode delegates NIM model types to lancor crate, not hardcoded
- **Profile variables inherit from `[variables]`** — profile vars are merged on top, not replacing
- **No runtime daemon** — aegis is a one-shot CLI; no background process or file watcher

## NIM Model Catalog (via aegis-opencode)

Aegis generates oh-my-opencode routing configs for 15+ NIM models. The model list is driven by `lancor` crate definitions. To add a model:
1. Update lancor crate model catalog
2. Run `aegis opencode generate` to regenerate

## CLI Reference

```bash
aegis init                          # Initialize manifest
aegis bootstrap [--profile NAME]    # Full system setup
aegis status [--json]               # Health check + drift summary
aegis link [--module NAME]          # Deploy configs
aegis unlink [--module NAME]        # Remove deployed configs
aegis diff [--module NAME]          # Show drift vs. deployed
aegis sync [--module NAME]          # Capture live → source
aegis opencode generate             # Generate OpenCode JSON
aegis toolchain install [TOOL]      # Install dirmacs tools
aegis toolchain status              # Show toolchain health
aegis profile list                  # List available profiles
```

## Environment

- No required env vars for core operations
- `NVIDIA_API_KEY` — needed only for `aegis opencode generate` if model validation calls NIM
- `RUST_LOG` — tracing log filter
