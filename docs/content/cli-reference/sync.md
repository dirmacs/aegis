+++
title = "aegis sync"
weight = 5
+++

# aegis sync

Capture live system state back into managed config sources.

## Usage

```bash
aegis sync [--module <NAME>]
```

## Behavior

For each sync rule defined in module manifests, aegis compares the live file to the managed source and copies changes back:

```toml
[[sync_rules]]
live_path = "~/.bashrc"
managed_path = "bashrc"
```

- **Symlinked configs**: skipped (changes already flow through the symlink)
- **Copied configs**: live content is copied back to the managed source
- **Template configs**: cannot reverse-sync (one-way rendering)

## Example

```bash
aegis sync
# ✓ Synced 2 config(s) from live system

aegis sync --module shell
# ✓ Everything in sync
```

After syncing, use `git diff` and `git commit` to version-control the captured changes.
