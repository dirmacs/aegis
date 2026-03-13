+++
title = "Overview"
weight = 1
+++

# Dirmacs Toolchain

Aegis has a built-in registry of all dirmacs ecosystem tools and can install, update, and health-check them.

## The ecosystem

| Tool | Crate | Description |
|------|-------|-------------|
| [Ares](https://github.com/dirmacs/ares) | `ares-server` | Agentic retrieval-enhanced server — multi-provider LLM orchestration, tool calling, RAG |
| [Daedra](https://github.com/dirmacs/daedra) | `daedra` | Web search MCP server powered by DuckDuckGo |
| [Thulp](https://github.com/dirmacs/thulp) | `thulp` | Execution context engineering platform — tool abstraction, MCP integration, workflows |
| [Eruka](https://github.com/dirmacs/eruka) | `eruka` | Context intelligence layer — schema-aware business context, gap detection |
| [Lancor](https://github.com/dirmacs/lancor) | `lancor` | Rust client for llama.cpp's OpenAI-compatible API |
| [Aegis](https://github.com/dirmacs/aegis) | `aegis-cli` | System configuration manager (this tool) |

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
