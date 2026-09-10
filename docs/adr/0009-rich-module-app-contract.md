# ADR 0009: Rich module application contract

**Language:** English | Français (follow-up)

> Date: 09/09/2026 · Status: **accepted** (lot 0 — contract and map only)  
> Tracking: [issue #150](https://github.com/azerothl/akasha-os/issues/150)

> Historical note: lots 1–5 subsequently landed the contract, Create package,
> navigation cutover, and generic composition primitives. See
> [create-contract.md](../create-contract.md) for the shipped surface.

## Context

The module runtime already ships signed `.aospkg` packages, a WASM guest, capability
review, `min_os_api`, transactional activation, package-local `declarative_ui`, and
module tool discovery.  Issue #149 (Tasks, lots 0–5 on `main`) proved that lifecycle
and declarative parity can move into an optional official app without a second SDK or
webview.

The declarative vocabulary today is intentionally small — sufficient for Notes and
Tasks, not for the native **Create / Image Studio** page.  Image Studio (~3.9k LOC in
`image_studio.rs`) owns a large parameter form, model catalogue coupling, composition
blocks, local preview/history, prompt enrichment, and asynchronous `media.image.*`
events entirely inside `aos-ui-egui`.

**External** means a **package** on the existing module runtime — not a cloud service,
not a process outside Akasha.

**Do not confuse** Create with platform services that stay in the host:

| Namespace | Role | Stays platform |
|-----------|------|----------------|
| `media.image.generate` / `cancel` / `upscale` | Image inference and job execution | Yes |
| `share/models/catalog-offerings.json` | Model catalogue and install registry | Yes |
| Agent `media.image.generate` tool | Agent image generation | Yes — works when Create uninstalled |
| `create.*` (proposed) | Create package business logic and documents | Becomes optional official app |

Lot 0 (this ADR) maps Create couplings and freezes the rich-app contract.  It does
**not** extract Image Studio, add `create_studio` to the host renderer, or implement
Lots 1–5.

## Decision

### 1. Ownership boundary

| Create package | Akasha platform / native host |
|----------------|-------------------------------|
| Declarative screen descriptions, locale resources (FR/EN), presets, prompt/parameter editing, history presentation, document migrations | Closed widget renderer, focus/a11y/theming, pointer gestures, layout, event dispatch |
| Create document schema and data under `/documents/create/**` | Permission enforcement, audited file picker/save/clipboard/notification services |
| Mapping user intent to `media.image.generate` request schema | Model catalogue, compatibility, GPU/CPU allocation, generation, durable job state, cancellation |
| Subscribing to job/resource events and reducing them into package state | Bounded subscriptions, event fan-out, cleanup on app close/uninstall |

An installed package never receives a capability because a widget names a tool or
service.  The host validates every reference at activation and enforces granted caps
on every invocation.

**No `create_studio` mega-widget.**  The host does not embed Create business logic in
the renderer.  Create ships as an `.aospkg` using generic primitives.

### 2. Versioned contracts

`manifest.min_os_api` remains the compatibility gate for host module APIs.
Rich apps add independently versioned manifest fields (frozen in
`crates/aos-proto/src/rich_app_contract.rs`, mirror:
[rich-app-contract.md](../rich-app-contract.md)):

```yaml
min_os_api: 1
ui:
  contract: 2
  document: ui/index.json
services:
  jobs: 1
  media_image: 1
```

| Version | Meaning |
|---------|---------|
| `min_os_api` | Host module runtime + bus API level (`OS_API_VERSION = 1`) |
| `ui.contract` | Closed declarative vocabulary major version (`1` = Tasks/Notes; `2` = rich app) |
| `services.jobs` | Generic job subscription facade |
| `services.media_image` | `media.image.*` action schema and event shape |

The host rejects a package whose major versions it does not support **before**
replacing the active install.  Staging is discarded on failure; the previous pointer
is retained (`module_rt::install`).

Legacy `ui.entry: ui/index.html` stays readable for v1 packages during migration.

### 3. State, events, actions, and resources

The host owns renderer state; the package owns local and document state via WASM.
The UI document declares bounded **state**, **bindings**, and **actions** — not an
arbitrary script engine.

**State** — typed slots with max counts and string lengths (see document limits below).

**Bindings** — module tool fetches into renderer state; invalidated by named changes
or successful actions, never by unbounded polling.

**Actions** — module tools (`"tool": "create.history.list"`) or platform services
(`"service": "media.image.generate"`) with explicit capability declarations.
Substitutions: `$local.*`, `$document.*`, `$row.*` only.  Visibility/enablement use
a typed predicate AST (max depth 8, 64 nodes).

**Events** — semantic interaction phases for native components:

```json
{"phase": "start|update|commit|cancel", "interaction_id": "…", "value": {}}
```

Pan/zoom/resize/drag stay local in native code; at most start/update/commit/cancel
reach the module.  No per-pointer-move WASM or bus round trip.

**Jobs** — generic handle:

```json
{"job_id":"…","kind":"media.image.generate","state":"queued|running|succeeded|failed|cancelled",
 "progress":{"completed":12,"total":30,"unit":"step"},"result":{},"error":null}
```

**Resources** — `image_view` owns texture cache, fit/zoom/pan, a11y, virtualized
galleries.  Packages supply authorized handles only.

**Uninstall** — must not delete `/documents/create/**`, shared models, or agents'
ability to generate images.

## Coupling map — Create / Image Studio

Each row is a **special coupling on `main` at lot 0**.  Destination names refer to
issue #150 lots unless marked **keep**.

### UI and navigation

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-ui-egui/src/main.rs` | `Tab::Image`, `image_studio: ImageStudioState`, `image_generating`, render `image_studio.ui()` | **Lot 2** — `Tab::Module("create")` via declarative UI |
| `crates/aos-ui-egui/src/nav.rs` | `TabKind::Create` → `Tab::Image`, primary rail `Ctrl+3` | **Lot 2** — module tab from package manifest |
| `crates/aos-ui-egui/src/i18n.rs` | ~64 `studio_*` keys, `tab_create`, status strings | **Lot 2** — `DeclUiLabels` in package (FR/EN only) |
| `crates/aos-ui-egui/src/guide.rs` | `GuideTopic::Create` ↔ `Tab::Image` | **Lot 2** — package help or generic guide hook |
| `crates/aos-ui-egui/src/image_studio.rs` | Full Create UI (removed lot 5) | **Deleted** — Create package + generic `layer_canvas` primitives |
| `crates/aos-proto/src/decl_ui.rs` | Create **not** in `PREINSTALLED_MODULES` / `NATIVE_UI_MODULES` | **Lot 2** — optional official app entry |

### Composition

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-proto/src/rich_composition.rs` | `RichLayer`, bounded undo, layer JSON contract | **Lot 5** — generic composition primitives |
| `crates/aos-ui-egui/src/rich_composition_ui.rs` | `layer_canvas`, `layer_list`, `undo_redo` host renderers | **Lot 5** — generic; `gallery-demo` sample |
| `crates/aos-ui-egui/src/image_composition.rs` | `CompositionBlock`, `InpaintMask`, `finalize_prompt_with_layout` | **Keep** — chat `/image` prompt injection; UI removed lot 5 |
| `crates/aos-ui-egui/src/runtime.rs` | Calls `finalize_prompt_with_layout` before `media.image.generate` | **Lot 2** — package action reducer; **keep** bus call in host |

### History

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-ui-egui/src/image_history.rs` | `ImageGenMeta`, `write_image_meta`, `list_image_history`, sidecar `*.meta.json` | **Lot 2** — `/documents/create/**` + `create.history.*` tools |
| `crates/aos-ui-egui/src/image_studio.rs` | `ui_image_history`, `apply_history`, `apply_history_for_path` | **Lot 2** — `image_view` gallery + binding restore |
| Storage today | `/downloads/*.meta.json` (40-entry cap) | **Migrate** — user docs under `/documents/create/` survive uninstall |

### Generation

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-ui-egui/src/runtime.rs` | `Cmd::MediaImage`, enrichment phases, `media.image.generate/cancel/upscale` bus | **Keep** platform service; **Lot 1** — generic job action adapter for packages |
| `crates/aos-ui-egui/src/cmd.rs` | `MediaImage`, `MediaImageCancel`, `Evt::MediaImageProgress`, `Evt::MediaOk` | **Lot 1** — generic job events |
| `crates/aos-ui-egui/src/media_event_controller.rs` | Routes progress/result into `image_studio` and chat | **Lot 1** — job subscription + package reducer |
| `crates/aos-model/src/media.rs` | Executes generation, writes `image-gen-progress.json` | **Keep** — platform inference |
| `crates/aos-model/src/bin/aos-modeld.rs` | `media.image.generate/cancel/upscale` handlers | **Keep** |
| `crates/aos-proto/src/lib.rs` | `MediaImageOptions`, `MediaImageGenerateRequest` | **Keep** — versioned via `services.media_image` |

### Models

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-ui-egui/src/models_page.rs` | `load_catalog_models`, `is_model_installed`, catalog tabs | **Keep** — platform catalogue |
| `crates/aos-ui-egui/src/image_studio.rs` | `refresh_catalog`, pack combos, inline install prompt | **Lot 2** — bind to catalogue via declared host service; install UX may stay platform |
| `crates/aos-ui-egui/src/models_controller.rs` | `on_model_download_finished` → `image_studio.on_download_finished` | **Lot 2** — generic catalog refresh event |
| `share/models/catalog-offerings.json` | Offering definitions | **Keep** |

### Chat

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-ui-egui/src/image_prompt.rs` | Enrichment kinds, system prompts, model gating | **Lot 2+** — Create package logic |
| `crates/aos-ui-egui/src/runtime.rs` | `enrich_image_prompt`, `enhance_image_prompt_chat` | **Lot 2** — declared LLM cap + package action |
| `crates/aos-ui-egui/src/main.rs` | `/image` slash → `Cmd::MediaImage` | **Keep** — chat integration independent of Create tab |
| `crates/aos-ui-egui/src/ui_chat_transcript.rs` | "Open in studio" → `Tab::Image` | **Lot 2** — deep link to installed Create module |
| `crates/aos-ui-egui/src/chat_media.rs` | Image render in transcript | **Keep** |
| `crates/aos-agent/src/tools.rs` | Static `media.image.generate` agent tool | **Keep** — agents unaffected by Create uninstall |

### Storage

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-ui-egui/src/decl_ui.rs` | `host_file_from_logical`, `try_load_png` | **Lot 1** — `image_view` resource loader |
| `var/run/create-presets.json` | Named presets (native) | **Lot 2** — `/documents/create/presets.json` |
| `/downloads/image-*.png` | Generation output | **Keep** — platform service output path |
| `crates/aos-ui-egui/src/os_open.rs` | OS folder picker, `user_downloads_dir` | **Keep** — audited picker service for packages |

## Declarative UI primitive inventory

### Shipped today (`ui.contract: 1`)

`column`, `row`, `heading`, `text`, `markdown`, `stat_row`, `table`, `line_chart`,
`bar_chart`, `pie`, `scatter`, `form`, `button`, `select`, `radio`, `checkbox`,
`textarea`, `image`, `audio`, `empty_state`, `count_label`.

Plus #149 additions: `row_actions`, `refresh_binds`, `DeclUiLabels` (FR/EN).

### First-slice gaps (`params → generate → progress → result → preview → history → save`)

| Step | Native Create today | v1 decl_ui | Gap (lot 1+) |
|------|---------------------|------------|--------------|
| Params | 50+ fields in `ImageStudioState` | `form`, `textarea`, `select`, `checkbox` | `slider`, `number`, multi-select, model combo with install state |
| Generate | `Cmd::MediaImage` | `button` + module `tool` only | **service action** for `media.image.generate` |
| Progress | `ImageGenUiState` + file poll | none | `job` widget + subscription |
| Cancel | `MediaImageCancel` | none | `job.cancel` action |
| Result | `Evt::MediaOk` → preview path | static `image` bind | job result reducer |
| Preview | egui texture + zoom/pan/inpaint | static `image` | **`image_view`** (fit/zoom/pan, a11y) |
| History | `ui_image_history` + sidecar scan | `table` could list | thumbnail gallery, row restore action |
| Save | implicit `/downloads` | none | audited save/picker **host service** action |

### Frozen v2 additions (not implemented)

`slider`, `number`, `progress`, `job`, `image_view`, `split`, `scroll`, `tabs`,
`spacer`, `layer_canvas`, `layer_list`, `undo_redo`, plus `state`, `bindings`,
`actions`, predicate AST, interaction events.

## Frozen public contract

Canonical constants: `crates/aos-proto/src/rich_app_contract.rs`  
Generic rules: [docs/rich-app-contract.md](../rich-app-contract.md)  
Create package direction: [docs/create-contract.md](../create-contract.md)  
Tests: `rich_app_contract` unit tests + `ui_v1_widget_kinds_match_decl_ui`.

| Field | Frozen value |
|-------|----------------|
| UI contract v1 (shipped) | `1` |
| UI contract v2 (Create target) | `2` |
| Host renders up to | `1` (today) |
| `services.jobs` | `1` |
| `services.media_image` | `1` |
| Max nodes / depth / state / bindings / subscriptions | 2000 / 32 / 128 / 64 / 32 |
| Platform image methods | `media.image.generate`, `media.image.cancel`, `media.image.upscale` |
| Create documents (target) | `/documents/create/**` |
| Uninstall preserves | user documents, shared models, agent `media.image.generate` |

## What #149 already validated vs still open

| #149 capability | Status on `main` | Reuse for rich apps |
|-----------------|------------------|---------------------|
| Transactional install + rollback | **Landed** `module_rt::install` | Same path for Create `.aospkg` |
| `min_os_api` enforced at install | **Landed** | Extended with `ui.contract` / `services.*` (lot 1) |
| `user_removed` + preinstall respect | **Landed** | Create optional preinstall |
| Protected vs preinstalled split | **Landed** | Host policy only |
| Declarative UI validate at install | **Landed** (v1) | Extend for v2 schema (lot 1) |
| Row actions + `refresh_binds` | **Landed** | History restore, preset refresh |
| Module tool discovery (worker/salon) | **Landed** | `create.*` tools when installed |
| `ui.contract` / `services.*` manifest | **Frozen** (lot 0) | Enforced lot 1 |
| State / actions / job subscriptions | **Frozen** (lot 0) | Implemented lot 1 |
| `image_view` / `job` widgets | **Frozen** (lot 0) | Implemented lot 1 |

## Measurable budgets and gates

**Reference machine** (assumptions documented; not a performance claim today):
8-core x86_64, 16 GiB RAM, 1080p display, release Preview build.

| Scenario | Target |
|----------|--------|
| Local pan/zoom/resize while a job runs | p95 frame ≤ 16.7 ms; no sync module/bus round trip per pointer move |
| Action validation + local state commit | p95 ≤ 8 ms at published document limits |
| Targeted binding invalidation | p95 first paint ≤ 100 ms after local/module action (excl. service latency) |
| Job progress UI | first visible update ≤ 250 ms after service event; ≤ 10 updates/s per job |
| Resource cleanup | zero live subscriptions/textures after one event-loop turn on close/uninstall |
| Document limits | 2000 nodes, depth 32, 128 state slots, 64 bindings, 32 subscriptions |

Lot 1 harness: synthetic 2000-node validation, open/close subscription cleanup,
controllable job emitting progress/cancel/error/success.  Lot 2 acceptance: real
available image engine (not Preview stub).

## Generic platform extensions (later PRs — not lot 0)

1. **`ui.contract: 2` schema validation** — state, bindings, actions, predicates at install.
2. **Service actions** — `media.image.generate` from declarative UI with cap enforcement.
3. **`job` widget + subscription** — progress/cancel without Create-specific events.
4. **`image_view`** — fit/zoom/pan, gallery, semantic interaction events.
5. **Audited save/picker service** — packages cannot embed raw host paths.
6. **Create `.aospkg`** — WASM + declarative UI (lot 2).
7. **Composition primitives** — `layer_canvas` / `layer_list` / `undo_redo` (lot 5, landed).

## Designer + supervisor surface locks (#150)

Locked for all lots (Lot 1 applies now; Lot 2+ must not regress):

| Lock | Rule |
|------|------|
| Create rail/tab | Human **Créer** / **Create** from host i18n or package `DeclUiLabels` — never the module id `create` in chrome |
| Host renderer | **No `create_studio` mega-widget** — Create ships as generic v2 primitives only |
| Tool discovery / chrome | Human names only in chat, roster, and module chrome — never raw ids such as `create.history.*`, `media.image.*`, or undiscovered module tool ids |
| Jobs / progress | Human-facing states in UI and transcript — never technical `job_id`, service names, or raw state tokens |
| Uninstall Create (Lot 2+) | Keep user data under `/documents/create/**`; agent `media.image.generate` remains available without Create installed |
| Lot 1 sample | **`gallery-demo` is not Create** — independent harness; sidebar title from document labels (`Démo galerie` / `Gallery Demo`) |

Lot 1 implementation notes:

- `job` widget paints localized state via `i18n::job_state_human_label` (FR/EN).
- Declarative panel errors use generic copy — internal service/tool ids are not shown.
- Module sidebar titles come from `DeclUiDocument::catalogue_title()`, not package `name`.

## Consequences

- Lot 0 PR is documentation + contract constants/tests only; runtime behaviour unchanged.
- Reviewers can trace every Create special case to a future lot or an explicit **keep**.
- Lot 1 handoff: implement v2 primitives + validation + benchmark harness + unrelated
  sample app demonstrating layout, `image_view`, job subscription, and state bindings.
- No host Create extraction, no `create_studio` widget, no inference rewrite.

## Out of scope (issue #150)

Marketplace, second SDK, webview HTML/JS, inference engine rewrite, extracting all
apps, full A/V support, composition/inpaint (first slice), measured startup/binary
claims in lot 0.
