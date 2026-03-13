+++
title = "Overview"
weight = 1
+++

# OpenCode Configuration

Aegis generates `opencode.json` and `oh-my-opencode.json` from typed TOML definitions. This gives you:

- **Type safety** — Rust structs validate the schema at generation time
- **No raw JSON editing** — define models, providers, and agents in clean TOML
- **Template support** — use variables for API keys and per-machine settings
- **Model references** — define a model once, reference it by key in agents and categories

## How it works

```
opencode.toml  ──[parse]──>  Rust structs  ──[transform]──>  JSON
                                                              ├── opencode.json
                                                              └── oh-my-opencode.json
```

The TOML file lives in your ai-tools module (typically `modules/ai-tools/opencode.toml`) and contains both the `[opencode]` and `[oh_my_opencode]` sections.

## Quick example

```bash
# Validate the TOML
aegis opencode validate

# Generate JSON files
aegis opencode generate

# Preview without writing
aegis --dry-run opencode generate
```

## Output format

The generated JSON matches the exact schema expected by [OpenCode](https://opencode.ai) and [oh-my-opencode](https://github.com/code-yeongyu/oh-my-opencode), including:

- Provider nesting with model definitions
- MCP server discriminated unions (`local` vs `remote`)
- Agent model references in `provider/model_id` format
- Schema URLs for validation
