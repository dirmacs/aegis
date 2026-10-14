+++
title = "Introduction"
sort_by = "weight"
template = "index.html"
+++

# Aegis

**Aegis** is a Rust CLI tool for system configuration management, built for the [dirmacs](https://github.com/dirmacs) ecosystem. It manages dotfiles, encrypted secrets, overlay networking, and orchestrates the dirmacs toolchain — all from declarative TOML manifests.

Think of it as a modern, Rust-native alternative to shell-script-based dotfile managers, purpose-built for AI-native development environments.

---

## Key Capabilities

- **Config Management** — deploy, sync, and diff your system configurations using symlinks, copies, or template rendering
- **Toolchain Management** — install, update, and health-check the dirmacs ecosystem (ares, daedra, thulp, eruka, lancor)
- **Multi-Machine Profiles** — define profiles for different machine types (dev VPS, workstation, CI) with module selection and variable overrides
- **Template Engine** — Tera-based variable substitution for config files that need per-machine customization

---

## Who is Aegis for?

| Audience | Use Case |
|----------|----------|
| **dirmacs developers** | Manage system configs across dev machines with first-class toolchain support |
| **DevOps / platform** | Reproducible machine provisioning with idempotent bootstrap and version tracking |

---

## Quick Reference

| Section | What You'll Find |
|---------|-----------------|
| [Getting Started](getting-started/installation/) | Installation, quickstart, first aegis.toml |
| [CLI Reference](cli-reference/init/) | Every command and flag |
| [Configuration](configuration/manifest/) | aegis.toml, module.toml, variables, profiles |
| [Toolchain](toolchain/overview/) | Managing the dirmacs ecosystem |
| [Guides](guides/nimakai/) | Model triage with nimakai, setting up a new machine |

---

## Architecture

Aegis is a 6-crate Rust workspace:

```
aegis/
├── aegis-core        # Manifest parsing, module system, templates, diffing
├── aegis-toolchain   # Dirmacs tool install, update, health checks
├── aegis-net         # Overlay network management (CA, peers, WireGuard, nftables)
├── aegis-secrets     # Encrypted secrets vault (passwords, API keys, tokens)
├── aegis-serve       # Authenticated HTTP secrets keychain service
└── aegis-cli         # Clap-based CLI binary + all subcommands
```

Built with: `clap`, `serde`, `toml`, `tera`, `tokio`, `similar`, `lancor`, and `console`.

---

## dirmacs ecosystem

Aegis is one part of the dirmacs open-source AI infrastructure stack:

| Project | Description |
|---------|-------------|
| [pawan](https://dirmacs.github.io/pawan) | Self-healing CLI coding agent — Rust-native, 34 tools, compiler-as-auditor |
| [ares-server](https://github.com/dirmacs/ares) | LLM runtime — multi-provider routing, tool coordination, RAG |
| [eruka](https://eruka.dirmacs.com) | Context memory engine — knowledge graph for agent session continuity |
| [deagle](https://github.com/dirmacs/deagle) | Code intelligence — tree-sitter + SQLite graph, 9 languages |
| [daedra](https://dirmacs.github.io/daedra) | Web search MCP server — 7 backends, automatic fallback |
