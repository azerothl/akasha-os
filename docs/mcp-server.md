# MCP server — expose Akasha to external IDEs

**Language:** English | [Français](fr/mcp-server.md)

Opt-in stdio [MCP](https://modelcontextprotocol.io/) façade so **Claude Code,
Codex, Cursor, Copilot**, and similar clients can call Akasha while Preview is
running. This is the reverse of Akasha’s existing MCP *client*
(`var/mcp/servers.yaml`).

Binary: `bin/aos-mcpd` (crate `aos-mcp`). Not started by `aos-session`.

## Prerequisites

1. Akasha OS Preview is running (bus listening — default `127.0.0.1:24701`).
2. Point the IDE at `aos-mcpd` from the Preview install (or a local `cargo
   build -p aos-mcp` output).

## Env

| Variable | Default | Meaning |
|----------|---------|---------|
| `AOS_BUS_ADDR` | `127.0.0.1:24701` | Intent bus host:port |
| `AOS_MCP_FROM` | `service:mcp` | `Intent.from` identity |

Logs go to **stderr** only (MCP forbids non-protocol bytes on stdout).

## Tools (MVP)

| Tool | Bus intent |
|------|------------|
| `akasha_models` | `model.list` |
| `akasha_infer` | `model.infer` (stream buffered into one text result) |
| `akasha_mem_stats` | `mem.stats` |
| `akasha_mem_context` | `mem.context` |
| `akasha_mem_list` | `mem.list` |
| `akasha_mem_recall` | `mem.user.recall` |
| `akasha_mem_remember` | `mem.user.remember` |

Out of scope for MVP: secrets, shell, FS, agent spawn/steer, notes/tasks modules.

## IDE config example

Claude Desktop / Cursor-style `mcp.json`:

```json
{
  "mcpServers": {
    "akasha": {
      "command": "C:/path/to/agentos-preview/bin/aos-mcpd.exe",
      "env": {
        "AOS_BUS_ADDR": "127.0.0.1:24701"
      }
    }
  }
}
```

On Linux/macOS use the `aos-mcpd` path without `.exe`. An example file also
lives at [`share/mcp/akasha-mcp.example.json`](../share/mcp/akasha-mcp.example.json).

## Protocol

Newline-delimited JSON-RPC 2.0 over stdio (`initialize`, `tools/list`,
`tools/call`, `ping`), protocol version `2024-11-05` — same framing as
Akasha’s MCP client in `aos-agent`.

## Related

- HTTP mem/secrets sibling: [sibling-bridge.md](sibling-bridge.md) (`aos-bridged`)
- Akasha as MCP *client*: `share/mcp/servers.yaml.example`
- Pull external coding CLIs *into* Akasha: Agents → `harness.run` / Runtime
  ([harness.md](harness.md))
