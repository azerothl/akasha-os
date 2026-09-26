# Dev Assistant P0 — module contract (Preview 0.19 / #247)

**Language:** English | Français (follow-up)  
**Status:** DA.1 bind + DA.2 search live; DA.3–DA.4 stubs  
**Tracking:** [issue #247](https://github.com/azerothl/akasha-os/issues/247)

## Goal

Enable a DeclUI + script **Dev Assistant** module for **large host projects**
without turning Preview into a full IDE (P1–P2 stay 0.20+).

## Host intents (P0)

| Intent | Lot | Status |
|--------|-----|--------|
| `workspace.bind` / `unbind` / `list` | DA.1 | **Live** — persists under `sessions/workspace-bindings.json`; returns caps `fs.read/write:/host/<id>/**` |
| `fs.search` / `code.search` | DA.2 | **Live** — bounded ripgrep (`rg`) with walk fallback; requires `fs.read:/host/<id>/**`; limit ≤ 200 |
| `fs.apply_patch` | DA.3 | Contract frozen (max 32 files / undo group); not executed yet |

Logical VFS root for a bind: `/host/<id>/…` mapped to the absolute host folder.

## Module contract (DA.4 — draft)

A Dev Assistant package should:

1. Call `workspace.bind` (user-confirmed host folder) and store `workspace_id`.
2. Request the returned caps through the normal cap-review path (fail-closed).
3. Use DeclUI v1 widgets only (forms / tables / textarea) — no `code_editor` yet.
4. Prefer `fs.search` then `fs.apply_patch` over full-file `fs.write` once DA.2–DA.3 ship.
5. Stay advisory by default; mutating tools require an explicit user goal (existing advisory kit).

## Non-goals (P1–P2)

`process.run`, `git.*`, LSP, DAP, DeclUI IDE widgets — see issue #247.

## Related

- Track A headless host: [#403](https://github.com/azerothl/akasha-os/issues/403) `aos-serverd`
- Host folder grants (one-off): `fs.host.access` (#157)
