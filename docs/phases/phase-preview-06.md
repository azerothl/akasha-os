# Phase P06 — Preview 0.6.0

**Language:** English | [Français](../fr/phases/phase-preview-06.md)

## Goal

Ship **Akasha OS Preview 0.6.0**: sibling-bridge schema export + HTTP↔bus
contract (E8), OS keyring wrap of the vault master key (E7, not TPM), and a
local signed module catalogue (E10). Not seL4 / bare metal. Not chat channels.
Not multi-GPU. Not a new P6 gate number. Not a sibling binary merge.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon B
(E8 / E7-keyring / E10). Sequencing: P06.1 → P06.2 → P06.3 ∥ P06.4 →
P06.5 (stretch) → P06.6.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P06.1 | E8 schemas | JSON Schema export of `mem.*` + `secrets.*` under `docs/bridge/` | done |
| P06.2 | E8 contract | HTTP JSON ↔ CBOR intent mapping in sibling-bridge (no live daemon) | done |
| P06.3 | E7 keyring | Master key in OS keyring (CredMan / Secret Service); file 0600 fallback | done |
| P06.4 | E10 catalogue | Signed local `share/modules/catalogue.yaml`; hash check on install; UI | done |
| P06.5 | Stretch | Chat Stop → `model.cancel`; clipboard copy (message / Troubleshoot) | done |
| P06.6 | Docs / ship | Phase docs, FEATURES/STATUS/TESTER, version 0.6.0, site, packaging | done |

Catalogue: [`docs/FEATURES.md`](../FEATURES.md).

## Exit gates

| Gate | Criterion |
|------|-----------|
| P06.1 | `docs/bridge/` holds `mem.*` + `secrets.*` schemas; `cargo test` fails if dump diverges |
| P06.2 | sibling-bridge maps HTTP JSON ↔ bus intents; non-goals (merge, channels) unchanged |
| P06.3 | After first-run, `master.key` is not a readable plaintext file; Settings secrets survive restart; headless Linux falls back to 0600; agent `secrets.get` denied |
| P06.4 | Install from catalogue → cap review + hash OK; tampered WASM refused; refuse caps → quarantine |
| P06.5 | If shipped: Stop interrupts a chat stream; Copy puts message text on the clipboard |
| Regression | `cargo test --workspace`; gates p4/p5 green on CUDA host |
| Packaging | Four artefacts (Win/Linux × CUDA/CPU) + complete `latest.json` |

## Out of scope

E7 TPM, live HTTP sibling daemon, sibling binary merge, assistant-as-module,
E9 / P5.2 multi-GPU, public marketplace, messaging channels, computer-use,
bare metal (E11–E13), ANN / F-MEM-04 eviction, PC cohort close, macOS,
automatic update apply, zip export.

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

Tag `v0.6.0` only when gates P06.1–P06.4 and P06.6 pass. PC cohort gate
remains independent. After 0.6: full TPM if the OS justifies it, a live
HTTP adapter only if a daemon is scheduled, E9 when a second GPU exists.
