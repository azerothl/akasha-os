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

## Optional Create and module tools (#439)

Additional OS tools are hidden and denied until the server configuration
supplies both an exact allowlist and explicit capabilities. MCP arguments
cannot change these grants, the actor, or the audit trace.

```json
"AOS_MCP_TOOLS": "akasha_image_generate;akasha_video_generate;notes.list",
"AOS_MCP_CAPS": "media.generate;fs.write:/downloads/**;tool.invoke:notes"
```

| MCP name | OS intent | Requirements |
|----------|-----------|--------------|
| `akasha_image_generate` | `media.image.generate` | `media.generate` + output `fs.write` |
| `akasha_video_generate` | `media.image.generate`, `sd_mode=vid_gen` | Same; installed video model/runtime |
| `akasha_image_upscale` | `media.image.upscale` | Same + input `fs.read` and installed upscaler |
| `akasha_module__notes__list` (example) | `module.invoke`, `notes.list` | Allowlisted OS name + `tool.invoke:notes`, installed and cap-reviewed Notes |

Module entries are derived from installed `module.describe` manifests. The
admitted read-only tools are `notes.list`, `notes.read`, `notes.search`,
`notes.links`, `notes.related`, and `tasks.list`. Missing modules are omitted.
Write/destructive tools, including `notes.create` (which can overwrite notes),
need a separate confirmation contract before exposure.

Image/video calls take `prompt`, optional `model_id`, `path`, and typed Create
`options`. Video defaults to 17 frames at 8 FPS; limits are 129 frames, 120 FPS,
2048 pixels per dimension and 100 steps. Input image paths (`init_image`,
`end_image`, `mask_image`) need scoped `fs.read`. Unknown options, path traversal
and caller-supplied capabilities/identity are refused. Model downloads are not
triggered by calls.

Results contain VFS paths and metadata. Calls use the normal bus handlers as
actor `mcp:<AOS_MCP_FROM>` with the server grants, without the service/human
permission bypass. Calls and refusals record `mcp.tool.call` audit events when
the audit service is reachable, plus refusal diagnostics on stderr. The current
stdio loop buffers one call at a time; it does not advertise progress or cancel.

Secrets, shell, arbitrary FS intents, agent spawn/steer, destructive module tools
and global media cancellation remain outside this façade.

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
