+++
title = "module.toml"
weight = 2
+++

# module.toml — Module Reference

Each module directory contains a `module.toml` that declares its packages, configs, hooks, and sync rules.

## Full example

```toml
[module]
name = "shell"
description = "Shell configuration (bash, starship, fzf)"

[[packages]]
name = "starship"
install_method = "cargo"
cargo_crate = "starship"
version_check = "starship --version"

[[packages]]
name = "fzf"
install_method = "apt"

[[configs]]
source = "bashrc"
target = "~/.bashrc"
strategy = "symlink"

[[configs]]
source = "starship.toml"
target = "~/.config/starship.toml"
strategy = "symlink"

[[hooks]]
event = "post-link"
command = "source ~/.bashrc"

[[sync_rules]]
live_path = "~/.bashrc"
managed_path = "bashrc"
```

## `[module]` section

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Module name (must match the name in aegis.toml) |
| `description` | string | Human-readable description |

## `[[packages]]` entries

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Package name (used for `which` detection) |
| `install_method` | `cargo` \| `apt` | yes | How to install |
| `cargo_crate` | string | — | Crate name if different from package name |
| `git_repo` | string | — | Git repo URL for `--from-source` installs |
| `version_check` | string | — | Command to get installed version |
| `expected_version` | string | — | Version substring to validate against |
| `features` | string array | — | Cargo features to enable |

## `[[configs]]` entries

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `source` | string | yes | Path relative to module directory |
| `target` | string | yes | Deployment path (`~` is expanded) |
| `strategy` | `symlink` \| `copy` \| `template` | — | Override the manifest default |

## `[[hooks]]` entries

| Field | Type | Description |
|-------|------|-------------|
| `event` | enum | `pre-link`, `post-link`, `pre-unlink`, `post-unlink`, `pre-bootstrap`, `post-bootstrap`, `pre-sync`, `post-sync` |
| `command` | string | Shell command to execute |

## `[[sync_rules]]` entries

| Field | Type | Description |
|-------|------|-------------|
| `live_path` | string | Path on the live system (`~` expanded) |
| `managed_path` | string | Path relative to the module directory |
| `ignore_patterns` | string array | Lines containing these strings are excluded from sync |
