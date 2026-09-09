# ADR 0009: Rich module application contract

**Language:** English · **Date:** 09/09/2026 · **Status:** accepted (lot 0)
Tracking: [issue #150](https://github.com/azerothl/akasha-os/issues/150)

## Context

The current module runtime already provides signed `.aospkg` packages, a WASM
guest, capability review, `min_os_api`, transactional activation, package-local
`declarative_ui`, and module tool discovery.  Its declarative vocabulary is
intentionally small and is sufficient for Notes and Tasks, but not for the
current native Image Studio.  Image Studio owns a large form, model catalogue
state, composition, local preview/history state and asynchronous media events
inside `aos-ui-egui`.

The objective is not to move that native page behind a `create_studio` widget.
It is to make the generic host contract sufficient for an independently
updatable Create package, while `media.image.generate` remains a platform
service usable by agents even when Create is absent.

This ADR is the lot-0 boundary.  It freezes responsibilities and API direction;
it deliberately does not claim that Create has been extracted.

## Decision

### Ownership boundary

| Create package | Akasha platform / native host |
|---|---|
| Declarative screen descriptions, locale resources, presets, prompt/parameter editing, history presentation and document migrations | Closed widget renderer, focus/a11y/theming, pointer gestures, layout calculation and event dispatch |
| Create document schema and package-private data under `/documents/create/**` | Permission enforcement, audited file picker/save/clipboard/notification services |
| Mapping user intent to the declared `media.image.generate` request schema | Model catalogue, compatibility, GPU/CPU allocation, image generation, durable job state and cancellation |
| Subscribing to declared job/resource events and turning them into package state | Bounded subscriptions, event fan-out, cleanup when an app closes/uninstalls |

An installed package never receives a capability merely because a widget names a
tool or a service.  The host validates every referenced tool and capability at
activation, and enforces the granted capability again on every invocation.

### Versioned contracts

`manifest.min_os_api` remains the compatibility gate for host APIs.  Rich UI
adds two explicit, independently versioned declarations to the manifest:

```yaml
ui:
  contract: 2
  document: ui/index.json
services:
  jobs: 1
  media_image: 1
```

`ui.contract` is a closed, monotonic vocabulary version.  The host rejects a
document whose major version it does not support *before replacing* the active
package.  A host may render a lower compatible version; a package must not use
an unknown widget/property as a feature probe.  `services.*` lists the minimum
major versions consumed by the package and is validated alongside
`min_os_api`.  The existing `ui/index.html` JSON location stays readable during
the migration; new packages use `ui/index.json` to make its data-only nature
unambiguous.

The runtime validates all of the following before activation:

1. manifest, archive fingerprint, WASM hash and the declared UI document;
2. UI vocabulary/version, tree limits, bindings and action schemas;
3. each referenced module tool against the package's declared tools;
4. each system-service action against an explicit declared capability; and
5. `min_os_api`, `ui.contract` and service-version compatibility.

The staging directory is discarded on failure and the previous active package
pointer is retained.

### State, bindings and actions

The host owns renderer state; a package owns its local and document state.  The
UI document may declare bounded bindings, state slots and actions, but cannot
evaluate arbitrary code or expressions.

```json
{
  "state": {
    "local": {"prompt": {"type": "string", "max_length": 8000}},
    "document": {"selected_history_id": {"type": "string", "nullable": true}}
  },
  "bindings": [{"id": "history", "tool": "create.history.list", "target": "/items"}],
  "actions": [{"id": "generate", "tool": "media.image.generate", "input": {"prompt": "$local.prompt"}}]
}
```

This is a contract shape, not an executable expression language.  `$local.*`,
`$document.*` and a row field are the only substitutions; they must resolve to
a schema-validated value.  Visibility, enablement and computed values use a
small typed predicate AST with a maximum depth and node count.  Bindings are
invalidated by named state/resource changes or a successful action, never by
polling an unbounded tree.

An action emits `{phase: start|update|commit|cancel, interaction_id, value}`.
The native component handles continuous pointer movement, zoom, panning and
resizing locally; it sends at most start/update/commit/cancel semantic events
to the module.  The module cannot make a pointer move wait on WASM or the bus.

### Jobs and resources

Long platform operations use a generic job handle rather than a Create-specific
widget:

```json
{"job_id":"…","kind":"media.image.generate","state":"queued|running|succeeded|failed|cancelled",
 "progress":{"completed":12,"total":30,"unit":"step"},"result":{},"error":null}
```

`job.cancel` is capability-gated and idempotent.  A job event subscription is
scoped to its owning application instance and is removed on app close,
uninstall, successful completion or explicit unsubscribe.  The durable job
service decides whether a job survives app close; Create's first slice uses
"continues in service, UI may reconnect", and uninstall never deletes the
result or user documents.

Media preview is generic: an `image_view` primitive owns decoded-texture cache,
fit/zoom/pan, keyboard access and virtualized thumbnail galleries.  A package
provides resource handles/paths only after host authorization; it does not read
arbitrary host paths from the renderer.

## Current coupling map

| Area | Current location | Extraction destination |
|---|---|---|
| Create state, form, preview, history and generation controls | `crates/aos-ui-egui/src/image_studio.rs` | Create package state/UI; generic layout, image view and forms in host |
| Composition blocks, selection and inpaint mask | `crates/aos-ui-egui/src/image_composition.rs` | generic canvas/layer primitives (lot 5) |
| Prompt enrichment and model-specific shaping | `crates/aos-ui-egui/src/image_prompt.rs` | Create package logic; model execution stays platform |
| Progress/result routing into Image Studio and chat | `crates/aos-ui-egui/src/media_event_controller.rs` | generic job subscription + package action reducer |
| Generation/cancel bus calls | `crates/aos-ui-egui/src/runtime.rs` | `media.image.*` service adapter exposed to permitted packages |
| Native navigation and Image Studio singleton | `crates/aos-ui-egui/src/main.rs` | `Tab::Module("create")` after parity |
| Image generation request/response protocol | `crates/aos-proto/src/lib.rs` | stays platform, gets a versioned job facade |
| Package UI validation/runtime | `crates/aos-proto/src/decl_ui.rs`, `crates/aos-platform/src/module_rt.rs` | extend generically; no Create-named branch |

## Measurable budgets and gates

These targets are measured on the reference Preview machine, documented with
hardware, resolution and release build in the PR that implements each lot.
They are gates, not current performance claims.

| Scenario | Target |
|---|---|
| local pan/zoom/resize while a job runs | p95 frame time ≤ 16.7 ms; no synchronous module/bus round trip per pointer move |
| action validation and local state commit | p95 ≤ 8 ms for a document at the published node/state limits |
| targeted binding invalidation | p95 first paint ≤ 100 ms after a successful local/module action, excluding service latency |
| job progress UI | first visible update ≤ 250 ms after service event; no more than 10 rendered updates/s per job |
| resource cleanup | zero live subscriptions and image textures owned by a closed/uninstalled app after one event-loop turn |
| declarative document limits | maximum 2,000 nodes, depth 32, 128 state slots, 64 bindings and 32 subscriptions per app instance |

The lot-1 test harness must include synthetic 2,000-node validation, repeated
open/close subscription cleanup, and a controllable generation job emitting
progress/cancel/error/success.  The lot-2 acceptance test must use a real
available image engine, not the Preview stub.

## Consequences and sequencing

1. **Lot 1:** add generic layout/image/job/state primitives, schema validation,
   compatibility checks and the benchmark harness; ship an unrelated sample
   application.
2. **Lot 2:** ship Create's parameters → real generation → progress/cancel →
   preview/history/save path as an `.aospkg`; errors retain entered parameters.
3. **Lot 3:** reuse the transaction, compatibility and cleanup rules for Tasks;
   preserve `/documents/tasks/**` and dynamic agent discovery.
4. **Lot 4–5:** migrate remaining Create capabilities and composition, then
   remove native Create branches only after functional/parity evidence.

Out of scope remains a webview, arbitrary scripting in the UI document, a new
package format, marketplace work, and changing the image inference engines.
