+++
title = "Quickstart"
weight = 2
+++

# Quickstart

Get from zero to a managed system configuration in 5 minutes.

## 1. Initialize

```bash
mkdir my-configs && cd my-configs
aegis init
```

This creates:
- `aegis.toml` — top-level manifest
- `modules/` — directory with shell, terminal, ai-tools, dev-tools, and dirmacs modules
- Each module gets a `module.toml` skeleton

## 2. Add a config

Edit `modules/shell/module.toml`:

```toml
[module]
name = "shell"
description = "Shell configuration"

[[configs]]
source = "bashrc"
target = "~/.bashrc"
strategy = "symlink"
```

Copy your current `.bashrc` into the module:

```bash
cp ~/.bashrc modules/shell/bashrc
```

## 3. Deploy

```bash
aegis link
# ✓ Linked 1 config(s) across 1 module(s)
```

Your `~/.bashrc` is now a symlink to `modules/shell/bashrc`. Edit either one and changes flow through instantly.

## 4. Check status

```bash
aegis status
```

This shows:
- Which packages are installed or missing
- Which configs are deployed, missing, or drifted
- Environment variable status
- Dirmacs toolchain health

## 5. Generate OpenCode configs

If you have an `opencode.toml` in your ai-tools module:

```bash
aegis opencode generate
# ✓ Generated ~/.config/opencode/opencode.json
# ✓ Generated ~/.config/opencode/oh-my-opencode.json
```

## Next steps

- [Configuration](../configuration/manifest/) — learn the full aegis.toml and module.toml format
- [CLI Reference](../cli-reference/init/) — every command and flag
- [OpenCode](../opencode/overview/) — deep dive into the TOML-to-JSON pipeline
