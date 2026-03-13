+++
title = "Providers & Models"
weight = 2
+++

# Providers & Models

## Defining a provider

```toml
[opencode.providers.nvidia]
npm = "@ai-sdk/openai-compatible"
name = "NVIDIA NIM"
base_url = "https://integrate.api.nvidia.com/v1"
api_key_env = "NIM_API_KEY"
```

| Field | Type | Description |
|-------|------|-------------|
| `npm` | string | NPM package for the provider SDK (default: `@ai-sdk/openai-compatible`) |
| `name` | string | Display name |
| `base_url` | string | API base URL |
| `api_key_env` | string | Environment variable holding the API key (never stored in TOML) |

The `api_key_env` is rendered as `{{env:VAR_NAME}}` in the generated JSON — opencode resolves it at runtime.

## Defining models

Models reference their provider by key:

```toml
[opencode.models.qwen3-5-122b]
provider = "nvidia"
model_id = "qwen/qwen3.5-122b-a10b"
name = "Qwen 3.5 122B MoE (Daily Driver)"
context_length = 262144
max_output = 16384
temperature = 0.6
top_p = 0.95
thinking = true
```

| Field | Type | Description |
|-------|------|-------------|
| `provider` | string | Provider key (must match a `[opencode.providers.X]` key) |
| `model_id` | string | API model identifier |
| `name` | string | Display name |
| `context_length` | integer | Context window size |
| `max_output` | integer | Maximum output tokens |
| `temperature` | float | Sampling temperature |
| `top_p` | float | Nucleus sampling |
| `top_k` | integer | Top-K sampling |
| `thinking` | boolean | Enable chain-of-thought / thinking mode |

## Setting the default model

```toml
[opencode.default_model]
model = "qwen3-5-122b"
```

This resolves to `nvidia/qwen/qwen3.5-122b-a10b` in the generated JSON.

## Generated output

Models are nested inside their provider in the JSON:

```json
{
  "provider": {
    "nvidia": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "NVIDIA NIM",
      "options": { "baseURL": "..." },
      "models": {
        "qwen/qwen3.5-122b-a10b": {
          "name": "Qwen 3.5 122B MoE (Daily Driver)",
          "limit": { "context": 262144, "output": 16384 },
          "parameters": { "temperature": 0.6, ... }
        }
      }
    }
  }
}
```
