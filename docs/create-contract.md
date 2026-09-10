# Create module public contract (shipped)

Implemented through lots 0–5 of [issue #150](https://github.com/azerothl/akasha-os/issues/150).
Generic rich-app rules: [rich-app-contract.md](rich-app-contract.md).

Create is an installable, preinstalled WASM package rendered by the generic
declarative host. The old native Image Studio and `Tab::Image` path were removed;
the primary rail opens `Tab::Module("create")`.

## Module

- **Name:** `create`
- **Package:** `share/modules/create.aospkg` (source: `modules/create/`)
- **UI contract:** `2` (`CREATE_TARGET_UI_CONTRACT`)
- **Navigation:** `Tab::Module("create")` (primary rail label: **Créer** / **Create**)

## Platform services (host-owned, not module tools)

Create maps user intent to these bus methods.  Agents and other apps use the same
API when Create is uninstalled.

Create accepts only a real engine response (`sdcpp` or another non-stub engine)
for both Image and Video output modes. Video requests use the same
`media.image.generate` service with `sd_mode=vid_gen`, frame count, and FPS.
The host Preview PNG fallback is reported as an error and is not added to
Create history.
The reference Preview smoke run is recorded in
[recette-preview-2026-09-09.md](recette-preview-2026-09-09.md) (512×512 `sdcpp`
generation and UI progress/cancel checks).

| Service | Bus method | Capability |
|---------|------------|------------|
| Image generation | `media.image.generate` | `media.generate`, `fs.write:/downloads/**` |
| Cancel | `media.image.cancel` | `media.generate` |
| Upscale | `media.image.upscale` | `media.generate`, `fs.write:/downloads/**` |

Proto types: `MediaImageGenerateRequest`, `MediaImageOptions`, `MediaGenerateResponse`
in `crates/aos-proto/src/lib.rs`.

## Module tools

| Tool id | Purpose |
|---------|---------|
| `create.history.list` | List generation history entries for the UI binding |
| `create.history.get` | Fetch one entry by id |
| `create.document.load` | Load package document state |
| `create.document.save` | Persist package document state |

Schemas are shipped with the WASM module and validated before activation.

## Storage

### Shared media artefacts

| Path | Content |
|------|---------|
| `/downloads/image-*.png` | Generated images |
| `/downloads/video-*.webm` | Generated videos (WebM) |
| `/downloads/*.meta.json` | Optional generation metadata sidecars |

### Create-owned durable state

| Path | Content |
|------|---------|
| `/documents/create/history.json` | Canonical history index |
| `/documents/create/state.json` | Package document state (selection, layout prefs) |
| `/downloads/image-*.png` | Generated artefacts (platform service output) |

**Uninstall rule:** removing the Create package must not delete `/documents/create/**`,
shared models under `share/models/`, or agents' ability to call `media.image.generate`.

### Capabilities (shipped manifest)

| Capability | Use |
|------------|-----|
| `fs.read:/documents/create/**` | Read package documents |
| `fs.write:/documents/create/**` | Write package documents |
| `fs.read:/downloads/**` | Preview generated images |
| `tool.invoke:create` | Invoke `create.*` module tools |
| `media.generate` | Declared service action for generation (host-enforced) |

## First-slice flow

Parameters → generate → progress → cancel → result → preview → history → save

Mapped to contract v2 primitives:

1. **Params** — `form`, `textarea`, `select`, `slider`, `number`, state slots
2. **Generate** — `action` → `media.image.generate`
3. **Progress** — `job` widget + subscription
4. **Result / preview** — `image_view` with authorized `/downloads/…` handle
5. **History** — `binding` → `create.history.list` + row restore action
6. **Save** — audited host file picker service (not raw path strings in UI)

## Extraction status

The former native coupling map is retained historically in [ADR 0009](adr/0009-rich-module-app-contract.md).
The shipped implementation is split across:

| Area | File |
|------|------|
| Generic declarative renderer | `crates/aos-ui-egui/src/decl_ui.rs`, `rich_decl.rs` |
| Create UI document | `modules/create/ui/index.json` |
| Create package logic | `modules/create/src/lib.rs` |
| Media generation bridge | `crates/aos-ui-egui/src/module_actions.rs`, `crates/aos-model/src/media.rs` |
| Composition / inpaint | `crates/aos-ui-egui/src/image_composition.rs` |
| Prompt enrichment | `crates/aos-ui-egui/src/image_prompt.rs` |
| History sidecars | `crates/aos-ui-egui/src/image_history.rs` |
| Generation commands | `crates/aos-ui-egui/src/runtime.rs` |
| Event routing | `crates/aos-ui-egui/src/media_event_controller.rs` |
| Model catalog | `crates/aos-ui-egui/src/models_page.rs` |
| Navigation | `crates/aos-ui-egui/src/main.rs`, `nav.rs`, `create_nav.rs` |
| i18n (~64 `studio_*` keys) | `crates/aos-ui-egui/src/i18n.rs` |

## Deliberate boundaries

- Video generation parity (native today; later lot)
- Generic composition canvas / layer list / undo-redo are shipped in Lot 5;
  Create-specific authoring semantics remain package-owned.
- Model catalogue ownership (stays platform — `share/models/catalog-offerings.json`)
- Chat prompt enrichment internals (Create may call chat LLM via declared caps)
