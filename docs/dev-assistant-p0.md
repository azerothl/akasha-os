# Dev Assistant P0 — module contract (Preview 0.19 / #247)

**Language:** English | Français (follow-up)  
**Status:** DA.1–DA.4 **live**  
**Tracking:** [issue #247](https://github.com/azerothl/akasha-os/issues/247)

## Goal

Enable a DeclUI + script **Dev Assistant** module for **large host projects**
without turning Preview into a full IDE (P1–P2 stay 0.20+).

## Host intents (P0)

| Intent | Lot | Status |
|--------|-----|--------|
| `workspace.bind` / `unbind` / `list` | DA.1 | **Live** — persists under `sessions/workspace-bindings.json`; returns caps `fs.read/write:/host/<id>/**` |
| `fs.search` / `code.search` | DA.2 | **Live** — bounded ripgrep (`rg`) with walk fallback; requires `fs.read:/host/<id>/**`; limit ≤ 200 |
| `fs.apply_patch` / `fs.undo_patch` | DA.3 | **Live** — replace_all or unified diff; multi-file undo groups in `sessions/workspace-patch-undo.json` |

Logical VFS root for a bind: `/host/<id>/…` mapped to the absolute host folder.

Agents may call these intents on the bus. Script modules may call the same
names via `host_call` (see [module-sdk.md](module-sdk.md)); caps still fail-closed.

## Module contract (DA.4)

A Dev Assistant package **must**:

1. Call `workspace.bind` (user-confirmed absolute host folder) and store
   `workspace_id` / `vfs_root` from the response.
2. Surface the returned `caps` (`fs.read/write:/host/<id>/**`) and require the
   normal **cap-review** path before search/patch (fail-closed; never mint caps
   inside the module).
3. Use **DeclUI v1** widgets only (`form`, `table`, `textarea`, `button`,
   `heading`, `text`, `column` / `row`) — **no** `code_editor`, `diff_view`,
   `file_tree`, or `problems_list` (P2).
4. Prefer `fs.search` / `code.search`, then `fs.apply_patch` (and
   `fs.undo_patch`), over full-file `fs.write` for host trees.
5. Stay **advisory by default**; mutating tools (`apply_patch`, `unbind`) only
   after an explicit user goal (existing advisory kit).

### Package layout (script + DeclUI v1)

```text
dev-assistant/
├── manifest.yaml      # name, tools, ui.mode=declarative_ui, required_caps
├── handlers.yaml      # ext-rt steps → workspace.* / fs.search / fs.apply_patch
└── ui/index.html      # declarative_ui JSON document (v1 widgets)
```

Reference source (MIT, not in the Preview zip catalogue by default):

[`community/modules/dev-assistant/`](../community/modules/dev-assistant/)

Scaffold/package/install with the usual Path A
([write-a-module.md](write-a-module.md)); copy or adapt the reference
handlers + UI. Name must not collide with bundled hosts (`notes`, `tasks`,
`canvas`, `ext-rt`, `create`).

### Recommended tools

| Tool | Maps to | Notes |
|------|---------|-------|
| `dev-assistant.bind` | `workspace.bind` | Form: absolute `host_path` |
| `dev-assistant.list` | `workspace.list` | Table of bindings |
| `dev-assistant.search` | `fs.search` | Requires read cap for `/host/<id>/**` |
| `dev-assistant.apply_patch` | `fs.apply_patch` | `replace_all` or unified diff hunks |
| `dev-assistant.undo_patch` | `fs.undo_patch` | Uses `undo_group_id` from apply |

Manifest `permissions.required_caps` may start empty (bind only) and grow after
cap review grants host globs; or declare the expected `/host/<id>/**` pattern
once the id is known to testers.

### DeclUI shape

- Bind form → refresh `dev-assistant.list`
- Search form (`root`, `query`) → table of hits (`path`, `line`, `preview`)
- Patch form (`path`, `diff` / replace) → show `undo_group_id`; undo button

## Non-goals (P1–P2)

`process.run`, `git.*`, LSP, DAP, DeclUI IDE widgets — see issue #247.

## Related

- Track A headless host: [#403](https://github.com/azerothl/akasha-os/issues/403) `aos-serverd`
- Host folder grants (one-off): `fs.host.access` (#157)
- Module authoring: [write-a-module.md](write-a-module.md), [module-sdk.md](module-sdk.md)
