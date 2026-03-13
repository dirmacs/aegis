+++
title = "aegis toolchain"
weight = 8
+++

# aegis toolchain

Manage the dirmacs ecosystem tools.

## Usage

```bash
aegis toolchain install [TOOL] [--from-source]
aegis toolchain status
aegis toolchain update [TOOL]
```

## Subcommands

### install

Install dirmacs tools via `cargo install`. Without a tool name, installs all tools.

| Flag | Description |
|------|-------------|
| `TOOL` | Specific tool name (ares, daedra, thulp, eruka, lancor) |
| `--from-source` | Build from git repo instead of crates.io |

### status

Show installed tools with versions and binary paths.

### update

Force-reinstall tools to get the latest version.

## Known tools

| Tool | Crate | Description |
|------|-------|-------------|
| ares | ares-server | Agentic retrieval-enhanced server |
| daedra | daedra | Web search MCP server |
| thulp | thulp | Execution context engineering platform |
| eruka | eruka | Context intelligence layer |
| lancor | lancor | llama.cpp client library |

## Example

```bash
aegis toolchain status
# Dirmacs Toolchain Status
#   ✓ daedra       daedra 0.1.6
#     /root/.cargo/bin/daedra
#   ✗ ares         not installed
#   1/5 tools installed

aegis toolchain install daedra
# ✓ Installed daedra

aegis toolchain install --from-source
# Builds all tools from their git repos
```
