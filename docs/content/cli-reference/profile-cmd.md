+++
title = "aegis profile"
weight = 9
+++

# aegis profile

List and inspect profiles.

## Usage

```bash
aegis profile list
aegis profile show <NAME>
```

## list

Shows all defined profiles with a `●` marker on the active one.

```
Profiles
  ● dev-vps — Development VPS with full tooling (5 modules)
    workstation — Local workstation with GPU (5 modules)
    ci — Minimal CI environment (2 modules)
```

## show

Displays a profile's modules and variable overrides.

```bash
aegis profile show workstation
# Profile: workstation
#   Local workstation with GPU
#
# Modules:
#   - shell
#   - terminal
#   - dev-tools
#   - ai-tools
#   - dirmacs
#
# Variables:
#   gpu_available = true
```

## Selecting a profile

Use the `--profile` global flag on any command:

```bash
aegis --profile ci status
aegis --profile workstation bootstrap
```

Or set the default in `aegis.toml`:

```toml
[aegis]
default_profile = "dev-vps"
```
