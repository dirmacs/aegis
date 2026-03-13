+++
title = "Core Concepts"
weight = 3
+++

# Core Concepts

## Manifest

The `aegis.toml` file at the root of your config repo is the **manifest**. It declares modules, profiles, and variables. Aegis searches upward from the current directory to find it.

## Modules

A **module** is a directory containing a `module.toml` and the config source files it manages. Modules are organized by domain — shell, terminal, ai-tools, dev-tools, dirmacs.

Each module defines:
- **Packages** — what to install (via cargo, apt, etc.)
- **Configs** — file mappings from source to target with a linking strategy
- **Hooks** — commands to run before/after linking
- **Sync rules** — how to capture live system state back

## Profiles

A **profile** selects which modules to enable and can override variables. Use profiles for different machine types:

```toml
[profiles.dev-vps]
modules = ["shell", "terminal", "dev-tools", "ai-tools", "dirmacs"]

[profiles.ci]
modules = ["shell", "dev-tools"]
```

## Linking Strategies

| Strategy | Behavior |
|----------|----------|
| `symlink` | Creates a symlink from target to source. Edits flow through instantly. Default. |
| `copy` | Copies the source to target. Use for files that applications modify at runtime. |
| `template` | Renders the source through Tera (`{{ variable }}`), writes as a regular file. One-way. |

## Variables

Variables provide dynamic values for templates:

```toml
[variables]
hostname = { source = "command", value = "hostname" }
user = { source = "env", value = "USER" }
gpu = { source = "static", value = "false" }
```

Profile-level variables override manifest-level ones.
