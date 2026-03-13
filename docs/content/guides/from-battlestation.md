+++
title = "Migrating from Battlestation"
weight = 1
+++

# Migrating from Battlestation

If you're coming from a bash-script-based dotfiles repo (like [battlestation](https://github.com/aeyecam/battlestation)), this guide maps the old approach to aegis.

## Conceptual mapping

| Battlestation | Aegis |
|---------------|-------|
| `setup.sh` | `aegis bootstrap` |
| `sync.sh` | `aegis sync` |
| Domain directories (`shell/`, `ai-tools/`) | Modules with `module.toml` |
| Raw JSON config files | Typed TOML with `aegis opencode generate` |
| Manual version tracking | `aegis status` + `aegis toolchain status` |
| Bash aliases for tool versions | Package specs with `version_check` |

## Step-by-step migration

### 1. Initialize

```bash
mkdir my-configs && cd my-configs
aegis init
```

### 2. Move config files

Copy your existing config files into the appropriate module directories:

```bash
cp ~/battlestation/shell/.bashrc modules/shell/bashrc
cp ~/battlestation/shell/aliases.sh modules/shell/aliases.sh
cp ~/battlestation/terminal/zellij/config.kdl modules/terminal/zellij/config.kdl
```

### 3. Declare configs in module.toml

For each config file, add a `[[configs]]` entry:

```toml
[[configs]]
source = "bashrc"
target = "~/.bashrc"
strategy = "symlink"
```

### 4. Declare packages

Instead of installing in `setup.sh`, declare packages:

```toml
[[packages]]
name = "starship"
install_method = "cargo"
cargo_crate = "starship"
version_check = "starship --version"
```

### 5. Convert opencode configs

Instead of maintaining raw `opencode.json`, create `modules/ai-tools/opencode.toml` with typed definitions. See the [OpenCode section](../opencode/overview/) for the full format.

```bash
aegis opencode generate
```

### 6. Deploy

```bash
aegis link
aegis status
```

### 7. Version control

```bash
git init && git add . && git commit -m "Migrate to aegis"
```

## What you gain

- **Type safety** for opencode configs (no more broken JSON)
- **Drift detection** (`aegis diff`) instead of guessing
- **Profile support** for multiple machines
- **Rust-native** toolchain management
- **Template variables** for per-machine config customization
