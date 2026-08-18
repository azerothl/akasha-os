# Phase P07 — Preview 0.7.0

**Language:** English | [Français](../fr/phases/phase-preview-07.md)

## Goal

Ship **Akasha OS Preview 0.7.0**: a **host-rendered declarative module UI**
(E15). Installed modules can open a real human surface (forms, tables, stats,
line charts) without a webview and without a new egui tab hardcoded in the
shell. Not seL4 / bare metal. Not `sandboxed_webview`. Not E13 compositor.
Not a new P6 gate number.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon B (E15).
Sequencing: P07.1 → P07.2 → P07.3 ∥ P07.4 → P07.5.

## Why this, not a webview

egui stays the shell (ADR 0003). Modules describe a **closed widget tree** in
`ui/index.html` (`type: declarative_ui`); the host paints it with existing
egui widgets + `egui_plot`. The WASM module still only runs tools. No HTML/JS
TCB, no Chromium/WebView2.

E13 (compositor / optional webview on bare metal) stays Horizon C.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P07.1 | E15 schema | Closed widget vocabulary + JSON Schema; unknown kinds refused | done |
| P07.2 | E15 host | Generic egui tab(s) for installed modules with `ui.mode=declarative_ui` | done |
| P07.3 | E15 bind | Host loads data via module tools; buttons/forms → `tool.invoke` (caps unchanged) | done |
| P07.4 | E15 authoring | `module.scaffold` / `package` write a real widget tree; agents can emit it | done |
| P07.5 | Docs / ship | Phase docs, FEATURES/STATUS/TESTER, version 0.7.0, site, packaging | done |

Catalogue (once shipped): [`docs/FEATURES.md`](../FEATURES.md).

### Widget vocabulary (closed)

`column`, `row`, `heading`, `text`, `markdown`, `stat_row`, `table`,
`line_chart`, `form` (fields from a tool input schema), `button` (invokes a
named tool). Optional `poll_ms` on the root for live refresh via the existing
scheduler/bus — stretch if it slips.

Notes and Tasks tabs stay **hardcoded** in 0.7 (no rewrite). New and
agent-created modules use the generic host.

## Exit gates

| Gate | Criterion |
|------|-----------|
| P07.1 | Schema published; host rejects unknown widget kinds (fail-closed, audited) |
| P07.2 | Installing a module with `declarative_ui` adds a human surface without editing `aos-ui-egui` tabs by hand |
| P07.3 | A table/chart on screen is bound to a module tool result; a button runs that tool under the same cap review as today |
| P07.4 | `module.scaffold` + `package` (script) produces a non-stub UI tree; an agent can create a module whose tab shows more than a title |
| P07.5 | FEATURES/STATUS/TESTER + version 0.7.0 |
| Regression | `cargo test --workspace`; gates p4/p5 green on CUDA host |
| Packaging | Four artefacts (Win/Linux × CUDA/CPU) + complete `latest.json` |

## Out of scope

`sandboxed_webview`, HTML/JS, CSS, video, tiled maps, iced/tauri, E13
compositor, rewriting Notes/Tasks onto the generic host, E7 TPM, live HTTP
sibling daemon, E9 / P5.2 multi-GPU, public marketplace, messaging channels,
computer-use, bare metal (E11–E13), PC cohort close, macOS.

## Build

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda   # GPU artefact
.\packaging\build-preview.ps1 -SkipModels -CpuOnly       # CPU artefact
```

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh
```

## Next

Tag `v0.7.0` only when gates P07.1–P07.5 pass. PC cohort gate remains
independent. **Next: Preview 0.8.0 / E16 + E17 + E15 widgets** — local image
+ audio (TTS), unified host, richer closed widget vocabulary
([phase-preview-08.md](phase-preview-08.md)). After 0.8: remaining
Horizon B (E7 TPM, live HTTP adapter if a daemon is scheduled, E9 when a
second GPU exists).
