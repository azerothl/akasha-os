# Dev Assistant commands and Code Studio adapter (#247)

**Source candidate, 18 October 2026.** P0 and read-only Git are already on main
(#407, #431). The chosen IDE direction is reuse of Akasha Code Studio through
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
It requires **host API 2** and **DeclUI contract 2**. Older modules retain their
API 1 / UI 1 compatibility.

`code_editor`, `diff_view`, `file_tree` and `problems_list` render host-supplied
data; they cannot read files, apply patches or execute commands themselves.
The patch form uses a monospaced editor. The reference file tree shows changed
paths returned by Git status, not every repository file. File/problem
selection stores a path; it does not implement full IDE navigation. Diffs
display at most 5,000 lines; editor text is limited to 64 KiB.

## Remaining acceptance work

- Real installed module → confirmation → command/commit UI flow; headless
  rendering and command tests alone do not establish native UI activation.
- Real Code Studio daemon integration, including project identity, index
  refresh, language-server availability and navigation. HTTP contract tests
  use a deterministic server and are not proof of installed Studio.
- Light DAP and multi-agent refactoring across subtrees remain outside this
  adapter slice. Do not mark #247 complete on the strength of these additions.
- Structural/symbol index quality, ignore/secret policy and very large
  repository evidence require the Code Studio delivery and its own tests.

Related: [ticket delivery log](tickets/open-ticket-delivery-2026-10-18.md),
[workspace contract](dev-assistant-p0.md), [MCP server](mcp-server.md).
