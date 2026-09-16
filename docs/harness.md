# External coding CLIs (harness)

**Language:** English | [Français](fr/harness.md)

How Akasha Preview launches **Codex**, **Claude Code**, and **Grok** CLIs
locally. Source of truth: `crates/aos-agent/src/harness.rs` and
`harness_backend.rs`.

This is **Akasha → CLI** (pull tools into agents). The reverse direction
(IDE → Akasha) is the optional MCP server — see [mcp-server.md](mcp-server.md).

## Two modes

| Mode | Where | Behaviour |
|------|--------|-----------|
| Tool `harness.run` | Native agent tool loop | One-shot spawn; result text returned to the model |
| External harness **Runtime** | Agents → Advanced → Runtime = Codex / Claude / Grok | Worker skips the native tool loop; sequential CLI turns under `aos-agent-worker` |

Both modes share the same spawn path (`run_turn`). Neither uses a shell.
Free-form `command` / `argv` / `args` are rejected.

## Prerequisites

1. Install the CLI so its binary is on the process **PATH** (`codex`, `claude`,
   or `grok`; on Windows also `.exe` / `.cmd` with a matching stem).
2. Enable the tool / capability **`harness.run`** on the agent (UI checkbox
   **External CLIs**, or auto-added when Runtime ≠ Native).
3. Optional working directory: absolute path, or relative to `AOS_HOME`
   (fallback: process cwd). Must be an existing directory.

## Tool arguments (`harness.run`)

| Field | Required | Notes |
|-------|----------|-------|
| `harness` | yes | `codex` \| `claude` (alias `claude-code`) \| `grok` (alias `grok-bot`) |
| `prompt` | yes | Max 24 000 characters |
| `cwd` | no | Resolved as above |
| `timeout_sec` | no | Default 180; clamped 15–600 |

Aliases in the harness name are accepted; anything else (including path tricks)
is refused.

## Fixed argv

Akasha never builds a shell line. It resolves the binary on `PATH`, then
spawns with a fixed argument vector.

### First turn / one-shot

| CLI | Argv |
|-----|------|
| Codex | `codex exec --skip-git-repo-check <prompt>` |
| Claude | `claude -p <prompt> --output-format text` |
| Grok | `grok -p <prompt>` |

### Continue / steer (Runtime backend)

| CLI | Argv |
|-----|------|
| Codex | `codex exec resume --last --skip-git-repo-check <prompt>` |
| Claude | `claude -c -p <prompt> --output-format text` |
| Grok | `grok -p <prompt>` (no stable resume flag — fresh prompt) |

## Runtime backend lifecycle

When `AgentSpec.execution_backend` is `ExternalHarness`:

1. **First turn** — goal statement as prompt, start argv above.
2. **Steer** — continue/resume argv with the directive (see table).
3. **Pause** — cancel flag kills the in-flight child; worker waits.
4. **Kill** — `aos-agentd` stops the worker; child uses `kill_on_drop`.

Turn timeout defaults to 180 s (same clamp as the tool). Goal `max_steps` /
`timeout_secs` still bound the worker loop.

If the agent is linked to a chat session, the **first** CLI spawn goes through
the act-gate (Allow / Deny), even in autonomous chat — same as the tool.

## Security notes

- Cap `harness.run` (or `harness.run:*`) required before any spawn.
- No shell, no arbitrary argv, no secrets tools via this path.
- Stdin is closed (`null`); stdout/stderr are captured and truncated in the
  tool result (≈32 k chars stdout).
- On Windows the child is created with `CREATE_NO_WINDOW`.

## Related

- Product catalogue: [FEATURES.md](FEATURES.md) §5 Agents
- MCP *into* Akasha: `share/mcp/servers.yaml.example`
- MCP *from* IDEs: [mcp-server.md](mcp-server.md)
