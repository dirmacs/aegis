+++
title = "MCP Servers"
weight = 3
+++

# MCP Servers

Define [Model Context Protocol](https://modelcontextprotocol.io) servers that opencode connects to.

## Local (stdio) MCP server

```toml
[opencode.mcp_servers.daedra]
type = "stdio"
command = "daedra"
args = ["serve", "--transport", "stdio", "--quiet"]
```

## Remote MCP server

```toml
[opencode.mcp_servers.context7]
type = "remote"
url = "https://mcp.context7.com/mcp"
```

## With environment variables

```toml
[opencode.mcp_servers.tavily]
type = "stdio"
command = "bunx"
args = ["tavily-mcp"]
env = { TAVILY_API_KEY = "$TAVILY_API_KEY" }
```

## With timeout

```toml
[opencode.mcp_servers.shannon-thinking]
type = "stdio"
command = "bunx"
args = ["server-shannon-thinking"]
timeout = 60
```

## Fields reference

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | `stdio` \| `remote` | yes | Transport type |
| `command` | string | stdio only | Binary to execute |
| `args` | string array | — | Arguments to pass |
| `url` | string | remote only | Server URL |
| `env` | table | — | Environment variables for the process |
| `enabled` | boolean | — | Enable/disable (default: `true`) |
| `timeout` | integer | — | Timeout in seconds |

## Generated output

Local MCP servers emit `command` as a single array:

```json
{
  "daedra": {
    "type": "local",
    "command": ["daedra", "serve", "--transport", "stdio", "--quiet"],
    "enabled": true
  }
}
```
