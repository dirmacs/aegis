+++
title = "Model Triage with Nimakai"
weight = 3
+++

# Model Triage with Nimakai

[Nimakai](https://github.com/dirmacs/nimakai) is a NVIDIA NIM model latency benchmarker written in Nim. Use it to find which models are responsive before configuring opencode and oh-my-opencode.

## Installation

### 1. Install the Nim toolchain

```bash
curl https://nim-lang.org/choosenim/init.sh -sSf | bash -s -- -y
export PATH="$HOME/.nimble/bin:$PATH"
```

Requires Nim >= 2.0.0 (tested with 2.2.8).

### 2. Install system dependencies

```bash
apt-get install -y libssl-dev
```

### 3. Build nimakai

```bash
git clone https://github.com/dirmacs/nimakai.git /opt/nimakai
cd /opt/nimakai
nimble build
```

### 4. Set your API key

```bash
export NVIDIA_API_KEY=$(grep NVIDIA_API_KEY ~/.config/opencode/.env | cut -d= -f2)
```

## Usage

### Quick benchmark

Run a single round against all models:

```bash
./nimakai list
```

This pings every model in the catalog and displays a table sorted by average latency, showing health status (UP, TIMEOUT, ERROR, NOT_FOUND) and verdict (Perfect, Slow, Unstable).

### Continuous monitoring

```bash
./nimakai roulette
```

Interactive TUI with live-updating metrics. Sort with keyboard: `A` (avg), `P` (p95), `S` (stability), `T` (tier), `N` (name), `U` (uptime).

### Model discovery

```bash
./nimakai discover
```

Compares live NVIDIA API models against the built-in catalog to find new models.

### Agent recommendations

```bash
./nimakai recommend
```

Suggests optimal model→agent assignments based on latency and capability.

## Interpreting results

| Verdict | Latency | Suitability |
|---------|---------|-------------|
| Perfect | <500ms | Any agent role |
| Normal | 500ms-1s | Most agent roles |
| Slow | 1-3s | Heavy tasks only (hephaestus) |
| Very Slow | 3-10s | Avoid for agents |
| Unstable | >10s | Do not use |

**Key insight**: Ping latency does not predict agent task completion time. A model at 300ms ping might complete an agent task in 2s, while a 370ms model takes 20s. The difference is tool-use capability. Always test models with actual OMO agent tasks after triaging with nimakai.

## Workflow: selecting models for aegis

1. Run `./nimakai list` to identify responsive models
2. Update `/opt/aegis/example/modules/ai-tools/opencode.toml` with responsive models
3. Assign agents to models based on role (see [Agents & Categories](@/opencode/agents.md))
4. Regenerate configs: `aegis opencode generate --input example/modules/ai-tools/opencode.toml`
5. Test agents sequentially: `npx oh-my-opencode run --agent <name> --directory <dir> "<prompt>"`
