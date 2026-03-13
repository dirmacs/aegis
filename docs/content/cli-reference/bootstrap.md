+++
title = "aegis bootstrap"
weight = 6
+++

# aegis bootstrap

Full system setup — install packages, deploy configs, and verify.

## Usage

```bash
aegis bootstrap [OPTIONS]
```

## Options

| Flag | Description |
|------|-------------|
| `--skip-packages` | Skip package installation phase |
| `--skip-configs` | Skip config deployment phase |

## Phases

1. **Packages** — iterates all modules, installs missing packages via their `install_method` (cargo, apt)
2. **Configs** — runs `aegis link` to deploy all config files
3. **Verify** — runs `aegis status` to confirm everything is in order

Bootstrap is idempotent — safe to re-run. Already-installed packages are skipped.

## Example

```bash
# Full bootstrap
aegis bootstrap
# ▸ Bootstrapping with profile: dev-vps
# Phase 1: Packages
#   ✓ 12 package(s) checked
# Phase 2: Configs
#   ✓ Linked 8 config(s) across 5 module(s)
# Phase 3: Verify
#   ...status output...
# ✓ Bootstrap complete

# Configs only (packages already installed)
aegis bootstrap --skip-packages
```
