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

**Key insight**: Ping latency does not predict agent task completion time. A model at 300ms ping might complete an agent task in 2s, while a 370ms model takes 20s. The difference is tool-use capability.

## Direct tool-use test

After nimakai identifies responsive models, verify tool-use capability with a direct API call before adding to OMO:

```bash
curl -sS --max-time 20 "https://integrate.api.nvidia.com/v1/chat/completions" \
  -H "Authorization: Bearer $NVIDIA_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "MODEL_ID_HERE",
    "messages": [{"role": "user", "content": "Read the file at /etc/hostname"}],
    "tools": [{"type": "function", "function": {"name": "read_file",
      "description": "Read file contents",
      "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}}],
    "max_tokens": 256, "stream": false
  }'
```

If the response contains a `tool_calls` array, the model can do agent work. If it returns plain text, it cannot.

## Known model quirks (2026-03-13)

| Model | Issue |
|-------|-------|
| MiniMax M2 | 410 Gone — decommissioned from NIM |
| MiniMax M2.1 | Pings OK but hangs on agent tool-use tasks |
| Kimi K2.5 | Intermittent timeouts — NIM server-side |
| Mistral Medium 3 | Fast ping (208ms) but cannot do tool-use — returns text |
| Nemotron Super 49B | Tool-use works via curl but too slow for OMO timeouts |
| Nemotron 3 Super | 1M context, agentic-optimized. Use `temperature=1.0, top_p=0.95` |
| Qwen 3.5 VLM | Correct model ID is `qwen/qwen3.5-397b-a17b`, not `qwen3.5-400b` |

## Workflow: selecting models for aegis

1. Run `./nimakai list` to identify responsive models
2. Run direct tool-use curl test (above) to verify agent capability
3. Update `/opt/aegis/example/modules/ai-tools/opencode.toml` with verified models
4. Regenerate: `aegis opencode generate --input example/modules/ai-tools/opencode.toml`
5. Test agents: `npx oh-my-opencode run --port 6000 --agent <name> --directory <dir> "<prompt>"`
6. Always use `--port 6000` or higher — default ports 4096-4100 get stuck from zombie servers
