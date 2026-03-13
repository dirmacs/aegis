+++
title = "aegis init"
weight = 1
+++

# aegis init

Initialize a new aegis manifest and module directory structure.

## Usage

```bash
aegis init [PATH]
```

## Arguments

| Argument | Default | Description |
|----------|---------|-------------|
| `PATH` | `.` | Directory to initialize |

## What it creates

```
<path>/
├── aegis.toml          # Top-level manifest with defaults
└── modules/
    ├── shell/module.toml
    ├── terminal/module.toml
    ├── ai-tools/module.toml
    ├── dev-tools/module.toml
    └── dirmacs/module.toml
```

## Example

```bash
mkdir my-dotfiles && cd my-dotfiles
aegis init
# ✓ Initialized aegis at ./aegis.toml
#   Created module directories under modules/
#   Edit aegis.toml and module.toml files to configure your system
```

Fails if `aegis.toml` already exists in the target directory.
