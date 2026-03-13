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
model = "qwen3-5-122b"
temperature = 0.6
top_p = 0.95
max_tokens = 32768

[oh_my_opencode.agents.oracle]
model = "qwen3-5-397b"

[oh_my_opencode.agents.librarian]
model = "glm4-7"

[oh_my_opencode.agents.explore]
model = "minimax-m2-1"
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | string | yes | Model key (resolved to `provider/model_id` in output) |
| `temperature` | float | — | Override model temperature |
| `top_p` | float | — | Override top_p |
| `max_tokens` | integer | — | Maximum output tokens |

## Defining categories

Categories map task types to models:

```toml
[oh_my_opencode.categories.deep]
model = "qwen3-5-397b"

[oh_my_opencode.categories.quick]
model = "minimax-m2-1"

[oh_my_opencode.categories.writing]
model = "qwen3-5-122b"
```

## Disabling hooks

```toml
[oh_my_opencode]
disabled_hooks = ["category-skill-reminder"]
```

## Generated output

Model keys are resolved to full `provider/model_id` format:

```json
{
  "$schema": "https://raw.githubusercontent.com/code-yeongyu/oh-my-opencode/master/assets/oh-my-opencode.schema.json",
  "disabled_hooks": ["category-skill-reminder"],
  "agents": {
    "sisyphus": {
      "model": "nvidia/qwen/qwen3.5-122b-a10b",
      "temperature": 0.6,
      "max_tokens": 32768
    }
  },
  "categories": {
    "deep": {
      "model": "nvidia/qwen/qwen3.5-397b-a17b"
    }
  }
}
```
