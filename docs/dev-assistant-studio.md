# Dev Assistant commands and Code Studio adapter (#247)

**Updated 19 October 2026.** P0 and read-only Git are on main (#407, #431).
The supervised command/widget/Studio adapter slice was merged in [#450](https://github.com/azerothl/akasha-os/pull/450).
The API 3 additions below were merged through [#451](https://github.com/azerothl/akasha-os/pull/451). The chosen IDE direction is reuse of Akasha Code Studio through
an adapter, confirmed by the user. This document does not claim that the #247
product-gap umbrella is complete or that a new Preview release has shipped.

## Supervised commands

| Intent | Required capabilities | Behaviour |
|---|---|---|
| `process.run` | `process.run:/host/<id>/**`, scoped read and write | Runs a named, administrator-configured task |
| `git.commit` | `git.commit:/host/<id>/**`, scoped read and write | Commits already staged files; stages nothing |

Every call needs a live host confirmation in the Control bar, with a 120-second
expiry. Payload identity/capabilities are replaced with the bus/module identity.
Attempts and outcomes are audited. No confirmation means no execution.

Place `workspace-process-policy.json` in the configured sessions directory:

```json
{
  "workspaces": {
    "my-project": {
      "check": {
        "program": "C:\\Users\\me\\.cargo\\bin\\cargo.exe",
        "args": ["check", "--locked", "--message-format=json"],
        "timeout_sec": 120
      }
    }
  }
}
```

Use the real executable path and binding ID. On Unix, use an absolute
executable path. The caller supplies only `root` and `task`, never an executable,
shell string, arguments or environment. Missing policies and unknown task names
fail closed. Commands run at the exact bound root.

Timeouts are limited to 1–300 seconds. Each output stream retains at most
256 KiB while draining the remainder. Owned Windows jobs / Unix process groups
stop descendants on exit or timeout. Cargo/rustc JSON diagnostics become at
most 500 Problems, with paths restricted to the bound workspace.

Git verifies its actual worktree root before committing, disables hooks,
fsmonitor and signing, and never stages, fetches or pushes. Host commands use
the user's account permissions. This is a confirmation and allowlist boundary,
**not an OS process/network sandbox**. `CARGO_NET_OFFLINE=true` affects Cargo;
it cannot stop arbitrary project build scripts from using the network.

## Code Studio adapter

Code Studio and the Akasha daemon remain separately installed and started.
The adapter reuses their project metadata, persistent code index and supported
LSP endpoint. It does not bundle a second Monaco editor or another daemon.

After creating/loading a project in Code Studio, bind the **same directory** in
Akasha OS and add `studio-adapter.json` beside the process policy:

```json
{
  "daemon_url": "http://127.0.0.1:3876",
  "ui_url": "http://127.0.0.1:5178",
  "workspaces": { "my-project": "existing-studio-project-id" }
}
```

Only numeric loopback HTTP origins are accepted. Redirects and HTTP proxies
are disabled. Every operation checks the configured project against
`GET /api/studio/projects` and compares canonical host paths before forwarding
any file contents. The guest cannot choose a daemon URL or project ID.
An unavailable daemon, changed directory or unsupported API returns an error.

| Intent | Capabilities in addition to scoped read | Delegated API |
|---|---|---|
| `studio.project` | none | Project metadata + `code-rag/status`; returns UI URL and exact project ID |
| `studio.reindex` | `studio.reindex:/host/<id>/**`, scoped write, host confirmation | Incremental `code-rag/reindex` (`force=false`) |
| `studio.lsp` | `studio.lsp:/host/<id>/**`, host confirmation | `lsp/query`: diagnostics, definition, references |

LSP needs a daemon version providing that endpoint and a separately installed,
allowed language server. Paths must resolve inside the workspace; document
text is limited to 256 KiB. The adapter does not install servers, apply LSP
renames or forward arbitrary protocol messages. Queries may start a server
process, hence the separate capability and confirmation.

The reference module shows the mapped project and index, and offers incremental
reindexing. Open the returned Code Studio URL and select the returned project
ID using **Load project**. Current Code Studio selects its own project; there
is no invented project URL parameter. Editing, project conversations and
semantic navigation remain in Code Studio. Its own permissions govern actions
initiated there; Akasha OS grants are not silently transferred to that daemon.
Loopback routing does not provide a network sandbox for that separate process.

## Reference package and native surfaces

[`community/modules/dev-assistant`](../community/modules/dev-assistant/) is a
source reference, not automatically installed in Preview. Package/install it
with [write-a-module.md](write-a-module.md) after building this host candidate.
Version 0.5.0 requires **host API 5**, **DeclUI contract 2**, and the rebuilt
ext-rt 1.1.0 WASM. API 1/2/3/4 and UI 1 modules remain supported.
The new handlers use explicit typed `{$arg: field}` references so optional
fields are omitted and JSON arrays/objects keep their types; template-looking
file contents are passed literally.

The source manifest intentionally requests no host access. Bind first, then put
only the chosen workspace grants in the local source manifest and repackage/update
it with explicit capability review, for example:

```yaml
permissions:
  required_caps:
    - fs.read:/host/my-project/**
    - fs.write:/host/my-project/**
    - debug.run:/host/my-project/**
```

Add `process.run`, `git.commit`, `studio.lsp`, `studio.reindex`,
`studio.memory.update`, `refactor.manage` or assigned `refactor.submit` scopes only
when needed. Granting an actor a capability does not silently grant it to an
installed WASM module. The host refuses approval of undeclared manifest grants.
A generic unconfigured reference can bind/list projects; other operations fail
closed until the package declares and receives its exact grants.

`code_editor`, `diff_view`, `file_tree` and `problems_list` render host-supplied
data; they cannot read files, apply patches or execute commands themselves.
The patch form uses a monospaced editor. The reference file tree shows changed
paths returned by Git status, not every repository file. File/problem
selection stores a path; it does not implement full IDE navigation. Diffs
display at most 5,000 lines; editor text is limited to 64 KiB.

## Project memory

`studio.memory` reads the mapped project's summaries and code-index status under
`studio-project:<project-id>`. It requires scoped `fs.read`.
`studio.memory.update` requires scoped read/write plus
`studio.memory.update:/host/<id>/**` and a host confirmation. It PATCHes exactly
one of `project_summary`, `evolution_summary`, or `policy_notes` (8 KiB maximum).
Runtime settings, verification commands and project IDs cannot be edited through
this intent. Persistence and structural indexing are provided by Code Studio;
this adapter does not create another embedding database or claim retrieval quality.

## Light DAP

Configure `workspace-debug-policy.json` beside the process policy, outside the
bound project. No launch is available by default:

```json
{
  "workspaces": {
    "my-project": {
      "python": {
        "adapter": {"program": "C:/Python/python.exe", "args": ["-m", "debugpy.adapter"]},
        "program": "main.py",
        "args": [],
        "stop_on_entry": true
      }
    }
  }
}
```

Install the chosen adapter separately. Its executable must be absolute and the
debuggee must be an existing file inside the bound project. `debug.start`
requires `debug.run:/host/<id>/**`, scoped read/write and confirmation showing
the fixed adapter/program arguments. The guest supplies only the configured name.

`debug.request` accepts `threads`, `stackTrace`, `scopes`, `variables`, `events`,
`continue`, `next`, `stepIn`, `stepOut`, `pause`, and `setBreakpoints` with their
explicit IDs/relative source path. `debug.stop` always permits the owner to clean
up, including after capability revocation. Sessions belong to an actor and
workspace, expire after 15 minutes and are limited to four. Frames are limited
to 1 MiB, events to 64 (oversized events truncated), stack traces to 50 frames,
variable pages to 100, and breakpoints to 64. Replies and event source paths are
mapped back to `/host/<id>`; outside source paths are redacted.

Attach, expression evaluation, variable mutation, `runInTerminal`, arbitrary DAP
messages and guest-provided executables/environment are unavailable. Debugging
uses the account's process rights; it is not a process or network sandbox.
`offline_strict` refuses launches/requests and switching mode stops owned debug
sessions. The reference native forms expose start, inspect/step and stop.

## Reviewed proposals over agent subtrees

`refactor.create` assigns 2..8 **existing distinct** `agent:<id>` identities to
disjoint existing relative directories. It requires scoped `refactor.manage`
and read capabilities. For example:

```json
{"root":"my-project","assignments":[
  {"part_id":"front","actor":"agent:front","subtree":"frontend"},
  {"part_id":"back","actor":"agent:back","subtree":"backend"}
]}
```

The response returns the plan ID and scoped grants to review; it neither starts
inference nor grants permissions. Each assigned actor calls `refactor.submit`:

```json
{"root":"my-project","plan_id":"returned-plan-id","part_id":"front","changes":[
  {"path":"frontend/main.ts","base_sha256":"sha256-of-current-UTF8-text","content":"complete proposed text"}
]}
```

Submission checks actor identity, subtree membership, `fs.read` and
`refactor.submit` for every target, and the current base hash. It writes proposals
only. Files must already exist, be UTF-8 and at most 64 KiB; up to 40 files per
part. Root/nested `.gitignore` and `.ignore` rules apply. Common build directories,
`.git`, `.env*`, key/certificate filenames and obvious private key bodies are
excluded. **This is not a complete secret scanner or a sandbox for the agents' other tools.**
Existing broad grants are not revoked by creating a plan.

The coordinator or `human:ui` calls `refactor.review` to inspect before/after
contents and a replacement diff, then `refactor.apply` with the exact returned
`review_sha256`. All parts must have submitted. Apply/undo require scoped
`refactor.manage`, read/write capabilities and a live host confirmation. Changing
a proposal invalidates the review fingerprint. The native module exposes review,
apply and undo forms; its diff display keeps the existing 5,000-line display cap.
The complete before/after proposal remains available in the tool response.

A durable journal at `sessions/workspace-refactor-plans.json` stores before-images
before host writes. Application checks all base files first, rechecks each file
before replacement, and attempts rollback on write failure. `refactor.undo` with
a fresh review fingerprint supports both completed and interrupted plans, refusing
to overwrite an external edit. This is optimistic multi-file application with
recovery, not a serializable filesystem transaction. Journals contain source
text in the local session directory: retain them for undo and review cleanup
manually. Limits: 2 MiB per applied before/after batch, 8 MiB journal, 16 retained plans.

## External MCP under offline-strict

Production worker/room discovery and tool calls query authoritative `net.status`.
Unknown/unavailable policy and `offline_strict` refuse host MCP. Discovery is
bounded to 30 seconds, tools to 60 seconds and protocol lines to 1 MiB. Active
calls and idle sessions recheck mode every second (policy calls time out after
three seconds). Refusal closes the owned Windows job/Unix process group; process
exit and session drop also clean up the group. A transition is detected within
the polling/policy deadline, not atomically at the network layer. Servers can
have account rights, and escaped processes are not contained by this mechanism.

Studio LSP/reindex are also refused in offline-strict because their separate
daemon can launch host tools. Read-only project metadata remains loopback-only.
The separate MCP *server* façade is documented in [mcp-server.md](mcp-server.md).

## Evidence and remaining acceptance work

See [the measured follow-up](tickets/dev-assistant-follow-up-2026-10-18.md) and
[its machine-readable evidence](tickets/evidence/2026-10-18-dev-assistant-validation.json).
Real debugpy stepping, real daemon indexing/LSP and memory across restart were
observed locally. WASM→host confirmations are exercised against the real module
runtime and filesystem, with deterministic proposal bodies.

Remaining: a human-operated freshly installed native module path on the target
OSes, semantic multi-agent refactoring with real models, retrieval/secret-scan
quality and representative large monorepos. The tested LSP daemon capability
still depends on [Akasha PR #169](https://github.com/azerothl/Akasha/pull/169),
which remains open. No upstream PR was merged as part of this follow-up.
The reference remains optional source, not a newly shipped Preview release.
#247, #434 and #435 remain open.

Related: [ticket delivery log](tickets/open-ticket-delivery-2026-10-18.md),
[workspace contract](dev-assistant-p0.md), [MCP server](mcp-server.md).

## Project-context follow-up (API 4)

The source reference now also exposes policy-filtered inspection, cached source
maps and explicit source reads. See [project policy and measured acceptance](dev-assistant-project-policy.md)
for scopes, bounds, source-map persistence, real-repository inspection and the
real-model proposal exercise. The earlier evidence above remains a dated record;
it does not claim that this new candidate is released.

## Resumable inspection (API 5)

The reference now includes `dev-assistant.inspect_continue` for a saved workspace
inspection. See [bounded pages and verification](dev-assistant-inspection-pages.md)
for cumulative limits, expiry, cache-policy compatibility and freshness.
