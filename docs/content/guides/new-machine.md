+++
title = "Setting Up a New Machine"
weight = 2
+++

# Setting Up a New Machine

Bootstrap a fresh system using your aegis config repo.

## Prerequisites

- Rust toolchain installed (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Git installed

## Steps

### 1. Install aegis

```bash
cargo install aegis-cli
```

### 2. Clone your config repo

```bash
git clone https://github.com/you/my-configs.git
cd my-configs
```

### 3. Set environment variables

Check what's needed:

```bash
aegis status
```

Set any missing env vars (API keys, etc.):

```bash
export NIM_API_KEY="your-key-here"
```

### 4. Bootstrap

```bash
aegis bootstrap
```

This will:
1. Install all declared packages via cargo/apt
2. Deploy all config files (symlinks, copies, templates)
3. Run a verification check

### 5. Choose a profile (optional)

If you have multiple profiles:

```bash
# See available profiles
aegis profile list

# Bootstrap with a specific profile
aegis --profile workstation bootstrap
```

### 6. Generate opencode configs

```bash
aegis opencode generate
```

### 7. Verify

```bash
aegis status
aegis toolchain status
```

Everything should show green checkmarks.

## Ongoing maintenance

```bash
# After editing configs on the live system
aegis sync
git add -A && git commit -m "sync: update configs"

# Check for drift periodically
aegis diff

# Update dirmacs tools
aegis toolchain update
```
