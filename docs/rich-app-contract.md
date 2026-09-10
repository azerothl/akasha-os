# Rich application contract

Frozen for [issue #150](https://github.com/azerothl/akasha-os/issues/150) lot 0.
Machine-checked in `crates/aos-proto/src/rich_app_contract.rs`.

This document names and versions the **generic** host contract for installable rich
apps.  Create is the reference consumer; see [create-contract.md](create-contract.md)
for package-specific paths and services.

## Compatibility gates

| Gate | Field | Current host | Rich-app target |
|------|-------|--------------|-----------------|
| Host module API | `min_os_api` | `1` (`OS_API_VERSION`) | Package must be ≤ host |
| UI vocabulary | `ui.contract` | `2` (v1 + rich widgets) | `2` |
| Job facade | `services.jobs` | `1` (validated for rich packages) | `1` |
| Image generation facade | `services.media_image` | `1` (validated for Create) | `1` |

Example manifest fragment (lot 2+ Create package):

```yaml
min_os_api: 1
ui:
  contract: 2
  document: ui/index.json
services:
  jobs: 1
  media_image: 1
```

The host validates manifest, archive fingerprint, WASM hash, UI document, tool
references, capability declarations, and all version gates **before** replacing an
active install.  On failure the staging directory is discarded and the previous
package pointer is retained (`module_rt::install` transactional flow from #149).

Legacy `ui.entry: ui/index.html` remains readable for v1 packages during migration.

## UI contract v1 (shipped)

Closed widget kinds — must match `decl_ui::WIDGET_KINDS`:

`column`, `row`, `heading`, `text`, `markdown`, `stat_row`, `table`, `line_chart`,
`bar_chart`, `pie`, `scatter`, `form`, `button`, `select`, `radio`, `checkbox`,
`textarea`, `image`, `audio`, `empty_state`, `count_label`.

Bindings call **module tools** only.  Row actions and `refresh_binds` landed via #149.

## UI contract v2 (shipped and enforced)

Adds bounded **state**, **bindings**, **actions**, and native primitives for rich
interaction.  Not an expression language or script engine.

### Additional widget kinds (lot 1+)

`slider`, `number`, `progress`, `job`, `image_view`, `split`, `scroll`, `tabs`, `spacer`,
`section`, `text_input`, `file_picker`.

`section` groups related controls with a visible heading. `text_input` binds a
single-line string slot and `file_picker` imports a user-selected image into the
module's logical `/downloads/**` space; the host never exposes an arbitrary host
filesystem path to the module.

### Composition widgets (lot 5+)

`layer_canvas`, `layer_list`, `undo_redo` — generic layer stack editing without
app-specific host branches.

| Widget | Purpose |
|--------|---------|
| `layer_canvas` | Normalized frame (default 16:9) with selection, drag, resize; semantic `commit` events |
| `layer_list` | Z-order list with visibility toggle and bounded reorder |
| `undo_redo` | Toolbar bound to a `canvas_id`; host undo depth ≤ 32 |

Local state keys (declared in the UI document):

```json
{
  "layers": [{"id": 1, "x": 0.1, "y": 0.2, "w": 0.3, "h": 0.3, "label": "A", "visible": true, "locked": false}],
  "selected_id": 1,
  "next_id": 2
}
```

Pointer moves stay host-local; at most `start` / `update` / `commit` / `cancel`
interaction phases reach the module (same contract as `image_view` pan/zoom).
Keyboard: **Cmd/Ctrl+Z** undo, **Cmd/Ctrl+Y** or **Cmd/Ctrl+Shift+Z** redo when
the canvas has focus.

Sample: `gallery-demo` **Composition** tab exercises all three widgets.

### State slots

```json
{
  "state": {
    "local": {
      "prompt": {"type": "string", "max_length": 8000}
    },
    "document": {
      "selected_history_id": {"type": "string", "nullable": true}
    }
  }
}
```

- **local** — ephemeral UI state (form fields, selection).
- **document** — persisted through the package WASM reducer and `/documents/<app>/**`.

### Bindings

```json
{
  "bindings": [
    {"id": "history", "tool": "create.history.list", "target": "/items"}
  ]
}
```

Bindings fetch module tool JSON into renderer state.  Invalidation is explicit:
named state/resource changes or a successful action — never unbounded polling.

### Actions

```json
{
  "actions": [
    {
      "id": "generate",
      "service": "media.image.generate",
      "input": {"prompt": "$local.prompt", "model_id": "$local.model_id"}
    }
  ]
}
```

Substitutions are limited to `$local.*`, `$document.*`, and `$row.*` (table rows).
Each resolves to a schema-validated value.  Visibility and enablement use a typed
predicate AST (max depth 8, max 64 nodes).

**Module tools** use `"tool": "module.tool.id"`.  **Platform services** use
`"service": "media.image.generate"` and require an explicit granted capability
(`media.generate`, `fs.write:/downloads/**`, …) — naming a service in the UI
document does not grant access.

### Semantic interactions

Native components (zoom, pan, resize, drag) emit structured events only:

```json
{"phase": "start|update|commit|cancel", "interaction_id": "…", "value": {}}
```

The host handles pointer movement locally.  No synchronous WASM or bus round trip
per mouse move.

## Jobs and resources

Generic job handle (not Create-specific):

```json
{
  "job_id": "…",
  "kind": "media.image.generate",
  "state": "queued|running|succeeded|failed|cancelled",
  "progress": {"completed": 12, "total": 30, "unit": "step"},
  "result": {},
  "error": null
}
```

- `job.cancel` is capability-gated and idempotent.
- Subscriptions are scoped to the owning app instance; removed on close, uninstall,
  completion, or explicit unsubscribe.
- Durable jobs may continue in the platform service after the app closes; uninstall
  never deletes user documents or shared models.

`image_view` owns decoded-texture cache, fit/zoom/pan, keyboard access, and
virtualized thumbnail galleries.  Packages supply authorized resource handles only.

## Document limits (per app instance)

| Limit | Value |
|-------|-------|
| Widget nodes | 2,000 |
| Tree depth | 32 |
| State slots | 128 |
| Bindings | 64 |
| Subscriptions | 32 |
| State string max length | 8,000 chars |
| Predicate depth | 8 |
| Predicate nodes | 64 |

## Performance budgets (gates for later lots)

Measured on the reference Preview machine (`8-core x86_64`, `16 GiB RAM`, `1080p`,
release build).  These are **targets**, not current performance claims.

| Scenario | Target |
|----------|--------|
| Local pan/zoom/resize during a running job | p95 ≤ 16.7 ms frame time; no per-move bus/WASM round trip |
| Action validation + local state commit | p95 ≤ 8 ms at document limits |
| Binding invalidation first paint | p95 ≤ 100 ms after local/module action (excl. service latency) |
| Job progress UI | first update ≤ 250 ms after service event; ≤ 10 updates/s per job |
| Resource cleanup | zero live subscriptions/textures after one event-loop turn on close/uninstall |

## Designer + supervisor surface locks (#150)

These rules apply to every lot; Lot 1 enforces them for generic v2 widgets and the
`gallery-demo` sample:

- **Create chrome (Lot 2+):** rail/tab label is human **Créer** / **Create**, never
  module id `create`.
- **No `create_studio`:** the host renderer stays generic; packages own business logic.
- **Human chrome only:** tool discovery, roster labels, and module chrome never show
  raw tool/service ids (`create.history.*`, `media.image.*`, …). Unknown discovered
  tools paint nothing rather than the id (see `i18n::tool_human_label`).
- **Human job copy:** `job` widgets and chat/transcript surfaces show localized progress
  states only — not `job_id`, `kind`, or wire-format state tokens.
- **Uninstall Create (Lot 2+):** documents under `/documents/create/**` remain; agents
  keep `media.image.generate` without the Create app.
- **Lot 1 sample:** `gallery-demo` validates the contract without Create semantics.

## What #149 already validated (reused by rich apps)

| Capability | Status on `main` |
|------------|------------------|
| Transactional install (stage → validate → activate; rollback) | **Landed** — `module_rt::install` |
| `min_os_api` rejection before activation | **Landed** |
| `user_removed` survives upgrade; preinstall respects uninstall | **Landed** |
| Protected vs preinstalled split (`is_protected_by_host`) | **Landed** (empty list today) |
| Declarative UI validation at install | **Landed** (v1 vocabulary) |
| Row actions + `refresh_binds` | **Landed** (#149 lot 2) |
| Module tool discovery for agents | **Landed** (#149 lot 3) |
| `ui.contract` / `services.*` manifest fields | **Landed** — validated before activation |
| State / actions / job subscriptions | **Landed** — generic renderer + scoped cleanup |
| `image_view` / `job` widgets | **Landed** — generic renderer |

## Not in this contract

- Marketplace, second SDK, webview HTML/JS
- Arbitrary scripting or expression evaluation in UI documents
- `create_studio` mega-widget embedding Create business logic in the host renderer
- Inference engine rewrite or full A/V parity
