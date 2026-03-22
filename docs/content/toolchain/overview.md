+++
title = "Overview"
weight = 1
+++

# Dirmacs Toolchain

Aegis has a built-in registry of all dirmacs ecosystem tools and can install, update, and health-check them.

## The ecosystem

### Core platform

| Tool | Crate/Binary | Description | VPS Status |
|------|-------------|-------------|------------|
| [Ares](https://github.com/dirmacs/ares) | `ares-server` | Agentic retrieval-enhanced server — multi-provider LLM orchestration, tool calling, RAG | `api.ares.dirmacs.com` LIVE |
| [Eruka](https://eruka.dirmacs.com) | `eruka-api` | Context intelligence layer — Eruka Tree (4-tier), schema-aware business context, gap detection, 13 MCP tools | `eruka.dirmacs.com` LIVE |
| [Doltares](https://github.com/dirmacs/doltares) | `doltares` | Orchestration daemon — workflows, self-healing, skill execution | `claw.dirmacs.com` LIVE |
| [Pawan](https://github.com/dirmacs/pawan) | `pawan` | CLI coding agent — StepFun Flash via NVIDIA NIM, task mode | `/usr/local/bin/pawan` LIVE |
| [Doltclaw](https://github.com/dirmacs/doltclaw) | `doltclaw` | Minimal Rust agent runtime — direct NIM calls, replacing openclaw | built |

### Supporting tools

| Tool | Crate/Binary | Description | VPS Status |
|------|-------------|-------------|------------|
| [Daedra](https://github.com/dirmacs/daedra) | `daedra` | Web search MCP server powered by DuckDuckGo | built |
| [Thulp](https://github.com/dirmacs/thulp) | `thulp` | Execution context engineering platform — tool abstraction, MCP integration, workflows | built |
| [Lancor](https://github.com/dirmacs/lancor) | `lancor` | Rust client for llama.cpp's OpenAI-compatible API | lib |
| [Nimakai](https://github.com/dirmacs/nimakai) | `nimakai` | NIM latency benchmarker (Nim lang, v0.9.1) | built |
| [Aegis](https://github.com/dirmacs/aegis) | `aegis-cli` | System configuration manager (this tool) | built |

### Frontends (Leptos WASM)

| Tool | Port | Description |
|------|------|-------------|
| dirmacs-site | 3200 | dirmacs.com Next.js marketing site |
| dirmacs-admin | static | Admin dashboard (Leptos WASM) |
| dotdot-v2 | static | DOT DOT marketplace (Leptos WASM) |
| enterprise-portal | static | Client portal template (Leptos WASM) |

### VPS system tools (non-dirmacs)

| Tool | Version | Description |
|------|---------|-------------|
| `rga` (ripgrep-all) | v0.10.10 | Grep through PDFs, DOCX, archives — uses poppler/pandoc adapters |
| `poppler-utils` | system | PDF text extraction (`pdftotext`) — required by rga |
| `pandoc` | system | Universal document converter — required by rga for DOCX/HTML |
| `caddy` | system | Reverse proxy + auto SSL (Let's Encrypt) |
| `zellij` | system | Terminal multiplexer |

## Quick commands

```bash
# Check what's installed
aegis toolchain status

# Install everything
aegis toolchain install

# Install a specific tool
aegis toolchain install daedra

# Build from source (git repos)
aegis toolchain install --from-source

# Update all to latest
aegis toolchain update
```

## How it works

Under the hood, `aegis toolchain install` runs `cargo install <crate>`. The `--from-source` flag switches to `cargo install --git <repo>`.

Tool detection uses `which` to find binaries, and version checks run each tool's `--version` flag.

## Module integration

The `dirmacs` module in your aegis config can also declare these as packages:

```toml
# modules/dirmacs/module.toml
[[packages]]
name = "daedra"
install_method = "cargo"
cargo_crate = "daedra"
version_check = "daedra --version"
```

This means `aegis bootstrap` will install them as part of the full system setup.
