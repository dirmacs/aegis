+++
title = "Introduction"
sort_by = "weight"
template = "index.html"
+++

# Aegis

**Aegis** is a Rust CLI tool for system configuration management, built for the [dirmacs](https://github.com/dirmacs) ecosystem. It manages dotfiles, generates typed OpenCode configurations, and orchestrates the dirmacs toolchain — all from declarative TOML manifests.

Think of it as a modern, Rust-native alternative to shell-script-based dotfile managers, purpose-built for AI-native development environments.

---

## Key Capabilities

- **Config Management** — deploy, sync, and diff your system configurations using symlinks, copies, or template rendering
- **OpenCode Generation** — generate `opencode.json` and `oh-my-opencode.json` from typed TOML definitions with full NVIDIA NIM model support
- **Toolchain Management** — install, update, and health-check the dirmacs ecosystem (ares, daedra, thulp, eruka, lancor)
- **Multi-Machine Profiles** — define profiles for different machine types (dev VPS, workstation, CI) with module selection and variable overrides
- **Template Engine** — Tera-based variable substitution for config files that need per-machine customization

---

## Who is Aegis for?

| Audience | Use Case |
|----------|----------|
| **dirmacs developers** | Manage system configs across dev machines with first-class toolchain support |
| **AI/ML engineers** | Generate and maintain complex OpenCode + oh-my-opencode configurations from clean TOML |
| **DevOps / platform** | Reproducible machine provisioning with idempotent bootstrap and version tracking |

---

## Quick Reference

| Section | What You'll Find |
|---------|-----------------|
| [Getting Started](getting-started/installation/) | Installation, quickstart, first aegis.toml |
| [CLI Reference](cli-reference/init/) | Every command and flag |
| [Configuration](configuration/manifest/) | aegis.toml, module.toml, variables, profiles |
| [OpenCode](opencode/overview/) | TOML-to-JSON pipeline for opencode configs |
| [Toolchain](toolchain/overview/) | Managing the dirmacs ecosystem |
| [Guides](guides/from-battlestation/) | Migrating from battlestation, setting up a new machine |

---

## Architecture

Aegis is a 4-crate Rust workspace:

```
aegis/
├── aegis-core        # Manifest parsing, module system, templates, diffing
├── aegis-opencode    # Typed TOML → opencode.json + oh-my-opencode.json
├── aegis-toolchain   # Dirmacs tool install, update, health checks
└── aegis-cli         # Clap-based CLI binary
```

Built with: `clap`, `serde`, `toml`, `tera`, `tokio`, `similar`, `lancor`, and `console`.
