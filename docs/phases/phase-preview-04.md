# Phase P04 — Preview 0.4.0

**Language:** English | [Français](../fr/phases/phase-preview-04.md)

## Goal

Ship **Akasha OS Preview 0.4.0**: typed memory graph + richer bootstrap,
encrypted secrets vault, module cap review, and packaging hygiene for CUDA
**and** CPU artefacts. Not seL4 / bare metal. Not chat channels. Not multi-GPU.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon B
(E6 / E7 / E10-lite). Sequencing: P04.1 → P04.2 ∥ P04.3 → P04.4 → P04.5.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P04.1 | E6 store | Typed relations `similar` / `updates` / `supersedes`; `mem.relate` / `neighbors` / `list` / `update` | done |
| P04.2 | E6 UX | Structured bootstrap; Memory tab list / edit / delete / supersede | done |
| P04.3 | E7-lite | Encrypted vault (`vault.enc`), Settings secrets, `secret_name` in modeld, MCP `${secret:…}` | done |
| P04.4 | E10-lite | Cap review on `module.install` (no auto-approve); `share/mcp/servers.yaml.example` | done |
| P04.5 | Docs / packaging | Phase docs, FEATURES/TESTER/STATUS, sibling-bridge, `latest.json` 4 assets | done |

Catalogue: [`docs/FEATURES.md`](../FEATURES.md).

## Exit gates

| Gate | Criterion |
|------|-----------|
| P04.1 | Relate two memories `similar`; `updates`/`supersedes` persist after restart |
| P04.2 | Replace a fact: recall + bootstrap see the new one; UI lists / edits / deletes |
| P04.3 | Set Brave key in Settings; vault not plaintext; agent cannot `secrets.get` |
| P04.4 | Install `.aospkg` → cap review confirm; refuse → quarantined / empty caps |
| Regression | `cargo test --workspace`; gates p4/p5 green on CUDA host |
| Packaging | Four artefacts (Win/Linux × CUDA/CPU) + complete `latest.json` |

## Out of scope

E9 / P5.2 multi-GPU, public marketplace, sibling binary merge, ANN / F-MEM-04
eviction, messaging channels, computer-use, bare metal (E11–E13).

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

Tag `v0.4.0` only when all gates pass. PC cohort gate remains independent.
