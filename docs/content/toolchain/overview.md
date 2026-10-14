+++
title = "Overview"
weight = 1
+++

# Dirmacs Toolchain

Aegis has a built-in registry of dirmacs ecosystem tools and can install, update, and health-check them.

## The ecosystem

### Core platform

| Tool | Crate/Binary | Description |
|------|-------------|-------------|
| [Ares](https://github.com/dirmacs/ares) | `ares-server` | Agentic retrieval-enhanced server — multi-provider LLM orchestration, tool calling, RAG |
| [Eruka](https://eruka.dirmacs.com) | `eruka-api` | Context intelligence layer — Eruka Tree (4-tier), schema-aware business context, gap detection |
| [Pawan](https://github.com/dirmacs/pawan) | `pawan` | CLI coding agent — StepFun Flash via NVIDIA NIM, task mode |

### Supporting tools

| Tool | Crate/Binary | Description |
|------|-------------|-------------|
| [Daedra](https://github.com/dirmacs/daedra) | `daedra` | Web search MCP server powered by DuckDuckGo |
| [Thulp](https://github.com/dirmacs/thulp) | `thulp` | Execution context engineering platform — tool abstraction, MCP integration, workflows |
| [Lancor](https://github.com/dirmacs/lancor) | `lancor` | Rust client for llama.cpp's OpenAI-compatible API |
| [Nimakai](https://github.com/dirmacs/nimakai) | `nimakai` | NIM latency benchmarker (Nim lang, v0.9.1) |
| [Aegis](https://github.com/dirmacs/aegis) | `aegis-cli` | System configuration manager (this tool) |

### System tools (non-dirmacs)

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

A module in your Aegis config can also declare these as packages:

```toml
# modules/my-platform/module.toml
[[packages]]
name = "daedra"
install_method = "cargo"
cargo_crate = "daedra"
version_check = "daedra --version"
```

This means `aegis bootstrap` will install them as part of the full system setup.
