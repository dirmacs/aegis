+++
title = "aegis opencode"
weight = 7
+++

# aegis opencode

Generate and validate OpenCode configuration files from typed TOML definitions.

## Usage

```bash
aegis opencode generate [OPTIONS]
aegis opencode validate [OPTIONS]
```

## Options

| Flag | Description |
|------|-------------|
| `--input <PATH>` | Path to opencode TOML file (default: `modules/ai-tools/opencode.toml`) |
| `--output <DIR>` | Output directory for generated JSON (default: `~/.config/opencode/`) |

## generate

Reads the opencode TOML, resolves model references, and writes:
- `opencode.json` — provider, model, MCP server, and plugin configuration
- `oh-my-opencode.json` — agent and category definitions (if `[oh_my_opencode]` section exists)

## validate

Parses the TOML and runs the generation pipeline without writing files. Reports any errors (missing provider references, invalid model keys, etc.).

## Example

```bash
# Generate to default location
aegis opencode generate
# ✓ Generated /root/.config/opencode/opencode.json
# ✓ Generated /root/.config/opencode/oh-my-opencode.json

# Preview without writing
aegis --dry-run opencode generate

# Validate only
aegis opencode validate
# ✓ Valid opencode configuration at modules/ai-tools/opencode.toml
```

See [OpenCode Configuration](../opencode/overview/) for the full TOML format reference.
