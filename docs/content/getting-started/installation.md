+++
title = "Installation"
weight = 1
+++

# Installation

## From crates.io

```bash
cargo install aegis-cli
```

## From source

```bash
git clone https://github.com/dirmacs/aegis.git
cd aegis
cargo install --path crates/aegis-cli
```

## Verify

```bash
aegis --version
# aegis 0.1.0
```

## Requirements

- **Rust** 1.85+ (edition 2024)
- **Linux** (primary target)
- **cargo** for toolchain management features
- **llama.cpp server** (optional, for lancor-powered AI features)
