+++
title = "aegis diff"
weight = 4
+++

# aegis diff

Show drift between managed source configs and their deployed targets.

## Usage

```bash
aegis diff [--module <NAME>]
```

## Behavior

For each config mapping in the active modules:

- **Symlinks**: checks if the symlink points to the correct source. If so, there's nothing to diff.
- **Copies/Templates**: performs a line-by-line diff between source and target, displayed as a colorized unified diff.
- **Missing**: reports configs that aren't deployed yet.

## Example

```
── shell ──
  ~ ~/.bashrc
    +export NEW_VAR="hello"
    -export OLD_VAR="goodbye"

── dev-tools ──
  ✗ ~/.config/bat/config (target missing)

✓ No drift detected (for modules with no changes)
```

Output uses colored `+` (green) and `-` (red) markers for added and removed lines.
