# Native agent project edits and tasks (#247)

The project host already implements patches, undo and named tasks. The native
agent now exposes `fs.apply_patch`, `fs.undo_patch` and `process.run` through
the same host services. This is a source change awaiting review, not a new
installed Preview release or completed development-assistant acceptance.

## Activation

Explicitly select the intended tools alongside the existing read-only context
tools. The unchanged default kit and a read-only workspace kit do not select
these operations. A matching explicit selection prefix also selects them.

```json
{
  "tools": ["workspace.inspect", "workspace.read", "fs.apply_patch", "fs.undo_patch", "process.run"],
  "caps": ["fs.read:/host/demo/**", "fs.write:/host/demo/**", "process.run:/host/demo/**"],
  "policy": {"fs_write": true, "net": "allow"}
}
```

Bind and authorize the project through the existing operator workflow first.
Selecting these tools adds no capability. Their arguments cannot supply an
actor, capabilities, trace ID, executable, shell, working directory or environment.
The bridge uses the executing agent's identity and runtime grants in both the
request and its transport. As with project reads, the bus client must be bound
to `agent:<id>`. Shared salon daemon clients are refused; per-member salon
transport remains separate work. This is logical identity consistency, not
new IPC authentication or operating-system isolation.

## Calls

```json
{"hunks":[{"path":"/host/demo/src/example.py","diff":"complete replacement source\n","replace_all":true}]}
```

`fs.apply_patch` also accepts unified diffs when `replace_all` is false.
It requires write grants on the affected paths and returns the host-generated
`undo_group_id`. All hunks must target the same bound project, with at most
32 hunks and 512 KiB of serialized input. Relative paths, parent traversal,
Windows drive paths and alternate stream syntax are refused. No path is
silently rebased. Read the current source before editing and verify the result.

```json
{"undo_group_id":"undo-<timestamp>-<pid>"}
```

`fs.undo_patch` delegates to the existing persisted undo store. The host checks
write access to every restored path; knowing an ID does not grant access.

```json
{"root":"demo","task":"verify"}
```

`process.run` resolves only an administrator-configured named task from
`<sessions>/workspace-process-policy.json`, described in
[project policy](dev-assistant-project-policy.md). The host requires the exact
project read/write/process grants and asks for confirmation before execution.
`fs_write:false` refuses all three operations. Agent `net:deny`, an offline
host or unavailable network policy refuses `process.run`: host programs run
with the user's account and are not network-sandboxed. Allowing local-only
model routing does not isolate these programs.

The bridge waits up to 430 seconds, covering the host's 120-second confirmation
window and maximum 300-second task. Transport failures or a lost/oversized
response report unknown execution state and `retry_safe:false`. They do not
assert that a process or patch was cancelled; inspect state before retrying.

Advisory goals refuse these operations in both worker and room entrypoints.
Conditional questions such as "Quels seraient les besoins…" remain advisory.
The existing tool allowlist, model authorization gate, host confirmation,
patch transaction/undo and audit checks remain in force.
Model-authored runtime actions also pass the selected catalogue and live
allowlist before dispatch; a legacy JSON response cannot enable an unoffered
`agent.spawn` or use `cap.request` outside the allowlist.

Long tool loops also retain the original and latest user request when working
memory is compacted. Older leading compaction summaries are replaced, not
permanently accumulated. This prevents the native chat template from receiving
a tool-only history with no user query; it does not reconstruct a user request
already lost in an older persisted state or prove that model-generated edits
are correct.

## Verification

`native_agent_workspace` is a production-host integration test. A real broker
and native bridge apply a two-file patch, refuse missing grants and denied
confirmation, execute a fixed harmless test task after fixture approval, and
restore the exact originals. Its task reruns only a helper test in the current
test executable; it does not execute generated project code.

```text
cargo test -p aos-platform --test native_agent_workspace --no-default-features --locked
```

The real-model probe uses an isolated generated Python project, the existing
93-case behavioral validator and Qwen3.5-4B Q4_K_M pinned by SHA-256. The
validator stays outside the writable workspace. Before fixture-only automated
task approval, it checks the exact file set, validator hash, allowed Python AST
and refactored behavior. The harness retains edited sources and separately
tries native undo; it does not author or repair the model's source changes.

Build the worker and example, configure the existing Python permission bridge
as in [real worker](dev-assistant-real-worker.md), then run:

```text
cargo build -p aos-agent --bin aos-agent-worker -p aos-platform --example workspace_real_model_edit_probe --no-default-features --locked
python scripts/verify-project-agent.py --probe <absolute-probe-executable> --worker <absolute-worker-executable> --modeld <absolute-modeld-executable> --model <absolute-Qwen-GGUF>
```

The host is online for the named task; model routing remains local-only.
Automated fixture confirmations cannot validate native UI clicks, installed
confirmation UX, representative monorepo quality or multiple-machine/GPU
acceptance. See the [observed results](tickets/project-agent-operations-2026-11-03.md).
#247, #439, #444, #460, #434 and #435 remain open, as does native acceptance.
