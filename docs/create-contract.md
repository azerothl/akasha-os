# Create module public contract (proposed)

Direction frozen for [issue #150](https://github.com/azerothl/akasha-os/issues/150) lot 0.
Generic rich-app rules: [rich-app-contract.md](rich-app-contract.md).

**Create is not yet a package.** Image Studio remains native (`Tab::Image` in
`aos-ui-egui`).  This document freezes the target contract so lot 1+ can implement
without reopening naming or platform boundaries.

## Module

- **Name:** `create` (proposed)
- **Package:** not shipped — sources will live under `modules/create/` (TBD)
- **UI contract:** `2` (`CREATE_TARGET_UI_CONTRACT`)
- **Target navigation:** `Tab::Module("create")` after declarative parity

## Platform services (host-owned, not module tools)

Create maps user intent to these bus methods.  Agents and other apps use the same
API when Create is uninstalled.

| Service | Bus method | Capability |
|---------|------------|------------|
| Image generation | `media.image.generate` | `media.generate`, `fs.write:/downloads/**` |
| Cancel | `media.image.cancel` | `media.generate` |
| Upscale | `media.image.upscale` | `media.generate`, `fs.write:/downloads/**` |

Proto types: `MediaImageGenerateRequest`, `MediaImageOptions`, `MediaGenerateResponse`
in `crates/aos-proto/src/lib.rs`.

## Proposed module tools (lot 2+)

| Tool id | Purpose |
|---------|---------|
| `create.history.list` | List generation history entries for the UI binding |
| `create.history.get` | Fetch one entry by id |
| `create.preset.list` | List named parameter presets |
| `create.preset.save` | Persist a preset |
| `create.preset.delete` | Remove a preset |
| `create.document.load` | Load package document state |
| `create.document.save` | Persist package document state |

Exact schemas ship with the WASM module in lot 2.  Lot 0 freezes ids and storage layout only.

## Storage

### Today (native Image Studio — not the target contract)

| Path | Content |
|------|---------|
| `/downloads/image-*.png` | Generated images |
| `/downloads/*.meta.json` | Sidecar metadata (`ImageGenMeta`) |
| `var/run/create-presets.json` | Named presets (host-local) |
| `var/run/image-gen-progress.json` | Progress ticker file |

### Target (lot 2+)

| Path | Content |
|------|---------|
| `/documents/create/history.json` | Canonical history index |
| `/documents/create/presets.json` | Named presets |
| `/documents/create/state.json` | Package document state (selection, layout prefs) |
| `/downloads/image-*.png` | Generated artefacts (platform service output) |

**Uninstall rule:** removing the Create package must not delete `/documents/create/**`,
shared models under `share/models/`, or agents' ability to call `media.image.generate`.

### Capabilities (target manifest)

| Capability | Use |
|------------|-----|
| `fs.read:/documents/create/**` | Read package documents |
| `fs.write:/documents/create/**` | Write package documents |
| `fs.read:/downloads/**` | Preview generated images |
| `tool.invoke:create` | Invoke `create.*` module tools |
| `media.generate` | Declared service action for generation (host-enforced) |

## First-slice flow (lot 2 acceptance)

Parameters → generate → progress → cancel → result → preview → history → save

Mapped to contract v2 primitives:

1. **Params** — `form`, `textarea`, `select`, `slider`, `number`, state slots
2. **Generate** — `action` → `media.image.generate`
3. **Progress** — `job` widget + subscription
4. **Result / preview** — `image_view` with authorized `/downloads/…` handle
5. **History** — `binding` → `create.history.list` + row restore action
6. **Save** — audited host file picker service (not raw path strings in UI)

## Coupling inventory (native today)

See [ADR 0009](adr/0009-rich-module-app-contract.md) for the full file-level map.
Primary locations:

| Area | File |
|------|------|
| Form, preview, history UI | `crates/aos-ui-egui/src/image_studio.rs` |
| Composition / inpaint | `crates/aos-ui-egui/src/image_composition.rs` |
| Prompt enrichment | `crates/aos-ui-egui/src/image_prompt.rs` |
| History sidecars | `crates/aos-ui-egui/src/image_history.rs` |
| Generation commands | `crates/aos-ui-egui/src/runtime.rs` |
| Event routing | `crates/aos-ui-egui/src/media_event_controller.rs` |
| Model catalog | `crates/aos-ui-egui/src/models_page.rs` |
| Navigation | `crates/aos-ui-egui/src/main.rs`, `nav.rs` |
| i18n (~64 `studio_*` keys) | `crates/aos-ui-egui/src/i18n.rs` |

## Not in this contract

- Video generation parity (native today; later lot)
- Composition canvas / inpaint (lot 5)
- Model catalogue ownership (stays platform — `share/models/catalog-offerings.json`)
- Chat prompt enrichment internals (Create may call chat LLM via declared caps)
