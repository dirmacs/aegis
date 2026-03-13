+++
title = "aegis status"
weight = 2
+++

# aegis status

Health check — shows what's installed, missing, or drifted.

## Usage

```bash
aegis status [OPTIONS]
```

## Options

| Flag | Description |
|------|-------------|
| `--json` | Output as JSON |
| `--module <NAME>` | Filter to a specific module |

## Output sections

1. **Profile** — active profile name and description
2. **Environment Variables** — which required env vars are set or missing
3. **Modules** — per-module breakdown:
   - Package install status with version
   - Config deployment status (ok, missing, drifted, error)
4. **Dirmacs Toolchain** — installed tools with versions and binary paths

## Example

```
▸ Profile: dev-vps — Development VPS with full tooling

Environment Variables
  ✓ $USER (user)
  ✗ $NIM_API_KEY (nim_api_key)

Modules
  ▸ shell
    ✓ starship — starship 1.24.2
    ✓ bashrc → ~/.bashrc — ok
  ▸ dirmacs
    ✓ daedra — daedra 0.1.6
    ✗ ares-server — not installed

Dirmacs Toolchain
  ✓ daedra — daedra 0.1.6
  ✗ ares — not installed
```
