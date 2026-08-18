# Phase P09 — Preview 0.9.0

**Language:** English | [Français](../fr/phases/phase-preview-09.md)

## Goal

Ship **Akasha OS Preview 0.9.0**: **mid-token device migrate** (E18). After
E17 (0.8), Settings gpu/cpu/auto and load-based auto can already switch
device by **cancelling** the in-flight `model.infer` and restarting
`aos-modeld`. 0.9 keeps the **same stream**: tokens already emitted stay on
screen; remaining tokens continue on the new device (CPU ↔ GPU) without a
user-visible cancel.

Depends on P08 / E17. Not seL4 / bare metal. Not a new P6 gate number.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon B (E18).
Sequencing: P09.1 → P09.2 → P09.3.

## Why after 0.8

0.8 must ship a unified artefact and a correct cancel-then-reload path
first. Copying or reconstituting KV / sampler state across llama.cpp
backends while a stream is open is a separate, fail-closed change: if
migrate cannot complete, fall back to the 0.8 cancel+restart (audited),
never duplicate or drop tokens silently.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P09.1 | E18 migrate | Placement Manager can move an active infer CPU ↔ GPU without aborting the stream; KV/state owned by Model Subsystem | planned |
| P09.2 | E18 policy | UI pin and `auto` (load / E16 VRAM pressure) use migrate when a stream is live; 0.8 cancel+restart remains the fallback | planned |
| P09.3 | Docs / ship | FEATURES/STATUS/TESTER, version 0.9.0, site, packaging | planned |

Catalogue (once shipped): [`docs/FEATURES.md`](../FEATURES.md).

## Exit gates

| Gate | Criterion |
|------|-----------|
| P09.1 | Start a long GPU (or CPU) completion; switch device mid-stream; reply continues with no Stop and no truncated “cancelled” turn |
| P09.2 | `auto` under VRAM pressure migrates without the tester hitting Stop; pin gpu/cpu still overrides; failed migrate → 0.8 fallback + audit |
| P09.3 | FEATURES/STATUS/TESTER + version 0.9.0 |
| Regression | `cargo test --workspace`; gates p4/p5 green on CUDA host |
| Packaging | Two tester artefacts (Win / Linux) + complete `latest.json` |

## Out of scope

Video generation, always-on microphone / STT, E7 TPM, live HTTP sibling
daemon, E9 / P5.2 multi-GPU, E13 compositor, PC cohort close, macOS, bare
metal (E11–E13). Speculative decode / multi-backend token fusion.

## Next

Tag `v0.9.0` only when gates P09.1–P09.3 pass. After 0.9: remaining Horizon B
(E7 TPM, live HTTP adapter if a daemon is scheduled, E9 when a second GPU
exists).
