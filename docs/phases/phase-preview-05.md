# Phase P05 — Preview 0.5.0

**Language:** English | [Français](../fr/phases/phase-preview-05.md)

## Goal

Ship **Akasha OS Preview 0.5.0**: opt-in auto extraction of durable facts
from chat turns into long-term memory (`mem.user.remember` + E6
`updates`/`supersedes`). Not seL4 / bare metal. Not chat channels. Not
multi-GPU. Not a new P6 gate number.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon B
(**E14**). Sequencing: P05.1 → P05.2 → P05.3 → P05.4 → P05.5.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P05.1 | E14 extract | Intent `mem.extract`; JSON prompt; secret filter; low-priority `model.infer` | done |
| P05.2 | E14 persist | Remember + auto_link; metadata `source=chat`; dedup ~0.92; audit | done |
| P05.3 | E14 UX | Settings opt-in (default OFF); post-turn hook; toast; Memory `chat` badge | done |
| P05.4 | E14 tests | Unit + integration; TESTER step 7d | done |
| P05.5 | Docs / ship | Phase docs, FEATURES/STATUS/TESTER, version 0.5.0, site, packaging | done |

Catalogue: [`docs/FEATURES.md`](../FEATURES.md).

## Exit gates

| Gate | Criterion |
|------|-----------|
| P05.1 | `mem.extract` on « je m’appelle X, je préfère le français » returns ≥1 valid JSON fact |
| P05.2 | Restart → recall / `mem.context` see the fact; contradictory 2nd turn creates `supersedes` |
| P05.3 | Default OFF: zero writes. ON: toast + Memory badge. Delete from UI removes the fact |
| P05.4 | Paste `sk-…` / `ghp_…` in chat → 0 remember; audit `filtered` |
| Regression | `cargo test --workspace`; gates p4/p5 green on CUDA host |
| Packaging | Four artefacts (Win/Linux × CUDA/CPU) + complete `latest.json` |

## Out of scope

E7 TPM, E8 sibling binary merge, E9 / P5.2 multi-GPU, public marketplace,
messaging channels, computer-use, bare metal (E11–E13), pre-store approval
queue, extract from agents/MCP, ANN / F-MEM-04 eviction, PC cohort close.

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

Tag `v0.5.0` only when all gates pass. PC cohort gate remains independent.
