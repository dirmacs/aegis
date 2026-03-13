+++
title = "aegis.toml"
weight = 1
+++

# aegis.toml — Manifest Reference

The top-level manifest file that declares your system configuration.

## Minimal example

```toml
[aegis]
version = "0.1.0"

[[modules]]
name = "shell"
path = "modules/shell"
```

## Full example

```toml
[aegis]
version = "0.1.0"
description = "My system configuration"
default_profile = "dev-vps"
strategy = "symlink"
secrets_backend = "env"

[variables]
hostname = { source = "command", value = "hostname" }
user = { source = "env", value = "USER" }
nim_api_key = { source = "env", value = "NIM_API_KEY" }

[profiles.dev-vps]
description = "Development VPS with full tooling"
modules = ["shell", "terminal", "dev-tools", "ai-tools", "dirmacs"]
variables = { gpu_available = "false" }

[profiles.ci]
description = "Minimal CI environment"
modules = ["shell", "dev-tools"]

[[modules]]
name = "shell"
path = "modules/shell"

[[modules]]
name = "terminal"
path = "modules/terminal"
```

## `[aegis]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `version` | string | **required** | Manifest version |
| `description` | string | — | Human-readable description |
| `default_profile` | string | — | Profile to use when no `--profile` flag is given |
| `strategy` | `symlink` \| `copy` \| `template` | `symlink` | Default linking strategy for all configs |
| `secrets_backend` | `env` | `env` | How secrets are resolved |

## `[variables]` section

Key-value pairs where keys are variable names and values specify the source:

```toml
[variables]
hostname = { source = "command", value = "hostname" }   # Run a command
user = { source = "env", value = "USER" }                 # Read env var
region = { source = "static", value = "us-east-1" }       # Literal value
```

Variables are available in template-rendered config files as `{{ variable_name }}`.

## `[profiles.<name>]` section

| Field | Type | Description |
|-------|------|-------------|
| `description` | string | Human-readable description |
| `modules` | string array | Which modules to activate |
| `variables` | table | Variable overrides (take precedence over top-level) |

## `[[modules]]` entries

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Module identifier |
| `path` | string | Path to module directory (relative to manifest) |
