# Phase P03 — Preview 0.3.0

**Language:** English | [Français](../fr/phases/phase-preview-03.md)

## Goal

Ship **Akasha OS Preview 0.3.0**: prove the OS thesis in the UI (caps +
inference metrics), widen the tester cohort (CPU-only path), add a
cap-gated agent scheduler, and a second dual-surface WASM module
(**tasks**). Not seL4 / bare metal. Not chat channels.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon A
(E1–E5). Sequencing: E5 → E4 → E1 → E2 ∥ E3.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P03.1 | E5 | GPU/model metrics in sidebar + Models tab (TTFT, tok/s, VRAM) | done |
| P03.2 | E4 | Caps surface: `cap.list`, revoke, audit | done |
| P03.3 | E1 | CPU-only boot + tiny first-run pack + CPU packaging | done |
| P03.4 | E2 | `schedule.*` intents, persist, caps, minimal UI | done |
| P03.5 | E3 | `tasks.aospkg` dual-surface + Tasks tab | done |

Catalogue: [`docs/FEATURES.md`](../FEATURES.md).

## Exit gates

| Gate | Criterion |
|------|-----------|
| P03.1 | After one infer, UI shows live TTFT + tok/s + VRAM for the loaded model |
| P03.2 | Tester sees caps for an active agent and can revoke a non-critical cap (audited) |
| P03.3 | Machine without NVIDIA starts Preview; local chat works (slow OK) |
| P03.4 | 1-minute schedule fires an agent; audit event; cancel stops future fires |
| P03.5 | Human creates a task in UI; agent `tasks.list` sees it; reverse also works |
| Regression | `cargo test --workspace`; existing p4/p5 gates green on CUDA host |
| Packaging | CUDA + CPU zip/tar publishable; `latest.json` |

## Out of scope

E6–E13, messaging channels, public marketplace, computer-use, sibling binary
merge, multi-GPU P5.2, bare metal.

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

Tag `v0.3.0` only when all gates pass. PC cohort gate remains independent.
