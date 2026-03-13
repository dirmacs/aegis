+++
title = "Agents & Categories"
weight = 4
+++

# Agents & Categories (oh-my-opencode)

The `[oh_my_opencode]` section in your opencode TOML generates `oh-my-opencode.json`.

## Defining agents

Agents reference models by their key in `[opencode.models.*]`:

```toml
[oh_my_opencode.agents.sisyphus]
model = "devstral-2-123b"
temperature = 0.6
top_p = 0.95
max_tokens = 32768

[oh_my_opencode.agents.oracle]
model = "glm5"

[oh_my_opencode.agents.explore]
model = "step3-5-flash"
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | string | yes | Model key (resolved to `provider/model_id` in output) |
| `temperature` | float | — | Override model temperature |
| `top_p` | float | — | Override top_p |
| `max_tokens` | integer | — | Maximum output tokens |

## Agent roster

| Agent | Role | Recommended model type |
|-------|------|----------------------|
| sisyphus | Primary coder | Fast coding model |
| sisyphus-junior | Secondary worker | Fast general model |
| hephaestus | Heavy engineering | Best coding model (slow OK) |
| oracle | Deep analysis | Thinking model |
| explore | Codebase search | Fastest available |
| librarian | Documentation | Medium model |
| prometheus | Plan builder | Thinking model |
| atlas | Plan executor | Thinking model |
| metis | Plan consultant | Fast general model |
| momus | Plan critic | Fast instruct model |
| multimodal-looker | Visual tasks | VL (vision-language) model |
| local | Default fallback | Fast coding model |

## Defining categories

Categories map task types to models:

```toml
[oh_my_opencode.categories.deep]
model = "glm5"

[oh_my_opencode.categories.quick]
model = "step3-5-flash"

[oh_my_opencode.categories.writing]
model = "kimi-k2-instruct"
```

## Disabling hooks

```toml
[oh_my_opencode]
disabled_hooks = ["category-skill-reminder"]
```

## Running agents

Agents are run via `npx oh-my-opencode run`. They must be run **sequentially** — concurrent runs cause port collisions.

```bash
export NVIDIA_API_KEY=$(grep NVIDIA_API_KEY ~/.config/opencode/.env | cut -d= -f2)

npx oh-my-opencode run \
  --agent sisyphus \
  --directory /opt/aegis \
  "Add a --version flag to the CLI"
```

Clean up stale servers between runs:

```bash
pkill -9 -f "opencode serve" 2>/dev/null; sleep 2
```

## Generated output

Model keys are resolved to full `provider/model_id` format:

```json
{
  "$schema": "https://raw.githubusercontent.com/code-yeongyu/oh-my-opencode/master/assets/oh-my-opencode.schema.json",
  "disabled_hooks": ["category-skill-reminder"],
  "agents": {
    "sisyphus": {
      "model": "nvidia/mistralai/devstral-2-123b-instruct-2512",
      "temperature": 0.6,
      "max_tokens": 32768
    }
  },
  "categories": {
    "deep": {
      "model": "nvidia/z-ai/glm5"
    }
  }
}
```
