+++
title = "Variables & Templates"
weight = 3
+++

# Variables & Templates

Aegis uses [Tera](https://keats.github.io/tera/) templates for config files that need per-machine customization.

## Defining variables

In `aegis.toml`:

```toml
[variables]
hostname = { source = "command", value = "hostname" }
user = { source = "env", value = "USER" }
llm_port = { source = "static", value = "8080" }
```

## Variable sources

| Source | Description | Example |
|--------|-------------|---------|
| `env` | Reads an environment variable | `{ source = "env", value = "HOME" }` |
| `command` | Runs a shell command, captures stdout | `{ source = "command", value = "hostname -f" }` |
| `static` | A literal string value | `{ source = "static", value = "us-east-1" }` |

## Profile overrides

Profile-level variables take precedence:

```toml
[profiles.workstation]
variables = { gpu_available = "true", llm_port = "8081" }
```

## Using variables in templates

Set a config's strategy to `template`:

```toml
[[configs]]
source = "ares-config.toml.tmpl"
target = "~/.config/ares/config.toml"
strategy = "template"
```

Then use Tera syntax in the source file:

```
[server]
host = "{{ hostname }}"
port = {{ llm_port }}

{% if gpu_available == "true" %}
[gpu]
enabled = true
{% endif %}
```

## Resolution order

1. Profile variables override manifest variables
2. Missing env vars resolve to empty strings (with a warning)
3. Failed commands resolve to empty strings (with a warning)
