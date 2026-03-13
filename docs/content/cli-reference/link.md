+++
title = "aegis link / unlink"
weight = 3
+++

# aegis link

Deploy managed configs to their target locations.

## Usage

```bash
aegis link [--module <NAME>]
aegis unlink [--module <NAME>]
```

## Behavior

**link** deploys configs using the strategy specified in each config mapping:

| Strategy | What `link` does |
|----------|------------------|
| `symlink` | Creates symlink from target to source. Backs up existing non-symlink files as `.aegis-backup`. |
| `copy` | Copies source to target, creating parent directories as needed. |
| `template` | Renders the source file through Tera with resolved variables, writes result to target. |

**unlink** removes deployed configs. If a `.aegis-backup` exists, it's restored.

## Hooks

Modules can define hooks that run before/after linking:

```toml
[[hooks]]
event = "pre-link"
command = "echo 'about to link'"

[[hooks]]
event = "post-link"
command = "source ~/.bashrc"
```

Supported events: `pre-link`, `post-link`, `pre-unlink`, `post-unlink`.

## Example

```bash
# Deploy everything
aegis link
# ✓ Linked 8 config(s) across 5 module(s)

# Deploy only shell configs
aegis link --module shell
# ✓ Linked 2 config(s) across 1 module(s)

# Dry run
aegis --dry-run link
# [dry-run] would symlink ~/.bashrc -> /path/to/modules/shell/bashrc
```
