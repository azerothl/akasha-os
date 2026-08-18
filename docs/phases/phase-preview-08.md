# Phase P08 — Preview 0.8.0

**Language:** English | [Français](../fr/phases/phase-preview-08.md)

## Goal

Ship **Akasha OS Preview 0.8.0**: local **image + audio (TTS)** generation
(E16); a **unified CPU/GPU host artefact** with UI + load-based device policy
(E17); plus a **cleanup / refactor pass** before tag. Same bus, caps, audit,
and Placement Manager as GGUF chat. Not seL4 / bare metal. Not video. Not
always-on voice. Not a cloud-only sidecar. Not a new P6 gate number.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon B
(E16, E17). Sequencing: P08.1 → P08.2 ∥ P08.3 ∥ P08.5 → P08.4 → P08.6 →
P08.7 → P08.8.

## Why this, not a hosted image API

egui stays the shell (ADR 0003). Diffusion and TTS **compete for VRAM** with
the loaded LLM (F-PLC-06 / F-PLC-09). Agents need an explicit `media.generate`
capability; outputs land under `/downloads` with an audit trail. A remote
OpenAI-compatible image endpoint may exist later as a *routed* backend
(`local_only` still wins for `secret` data) — it is not the default 0.8 path.

STT / 24/7 voice / chat channels stay with the sibling (anti-roadmap).

## Why unify CPU and GPU (E17)

Testers today download a **CUDA zip or a CPU zip**. Settings already expose
Inference **auto / gpu / cpu**, but it only applies on the **next full
boot**, and `auto` means “NVIDIA present?”, not machine load. A CUDA-linked
`aos-modeld` can fail to start without NVIDIA DLLs — that is why the split
exists.

Preview 0.8 ships **one artefact per OS**. `aos-session` probes hardware and
spawns a backend that is safe on that machine (CPU process has no CUDA DLL
dependency). Settings **gpu / cpu** restart `aos-modeld` in the current
session (cancel in-flight infer first; **seamless mid-token migrate is
P09 / E18**). **auto** is a Placement Manager
policy with hysteresis: GPU when VRAM allows; more RAM/CPU offload (or
cpu-only) under VRAM/CPU pressure or when E16 media needs the GPU; promote
back when pressure lifts. Pin `gpu` / `cpu` overrides auto.

`-CpuOnly` remains a **builder** escape hatch (no CUDA toolkit on the build
host), not a second tester download.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P08.1 | E16 registry | Image + TTS in the model catalogue / offerings; Placement Manager evicts LLM vs media shards | planned |
| P08.2 | E16 image | `media.image.generate` (prompt → PNG under `/downloads`); cap review; audit | planned |
| P08.3 | E16 audio | `media.audio.generate` (text → WAV/OGG TTS); same cap family; audit | planned |
| P08.4 | E16 surface | Chat shows the image / plays the clip; optional E15 `image` / `audio` widget kinds | planned |
| P08.5 | E17 device | One Win + one Linux artefact; Settings gpu/cpu/auto apply without reinstall; auto follows VRAM/CPU load (hysteresis) | planned |
| P08.6 | E16 packs | Optional media packs (download, not baked into the zip); GPU preferred for image | planned |
| P08.7 | Hygiene | Cleanup + refactor of Preview host crates (dead code, splits, naming); **no** behavior change | planned |
| P08.8 | Docs / ship | Phase docs, FEATURES/STATUS/TESTER, version 0.8.0, site, packaging | planned |

Catalogue (once shipped): [`docs/FEATURES.md`](../FEATURES.md).

### Placement

Media models are evictable shards, not a second unmanaged GPU client.
If VRAM cannot hold LLM + diffusion: unload or refuse with an alternative
(smaller pack / CPU TTS / explicit skip). CPU TTS is in scope; CPU image
generation may be slow or skipped with a documented refuse.

### Cleanup / refactor (P08.7)

A dedicated pass **after** E16/E17 lands and **before** tag, so new media and
device code is included. Scope is the Preview host (`crates/aos-*`, egui, packaging scripts
touched by 0.8) — not a seL4 rewrite, not Notes/Tasks onto E15, not a new UI
toolkit.

In scope: dead code and unused deps; oversized modules split along existing
service boundaries; duplicated helpers collapsed; intent / cap / file naming
aligned with `media.*`; leftover bilingual UI labels that drifted. Gate:
behavior-preserving (`cargo test --workspace` + p4/p5 unchanged). Prefer
hygiene commits separate from E16 feature commits.

## Exit gates

| Gate | Criterion |
|------|-----------|
| P08.1 | Catalogue lists at least one image pack and one TTS pack; loading media does not leak VRAM outside Placement Manager accounting |
| P08.2 | Prompt from chat or a module tool writes a PNG under `/downloads`; audited; agent without `media.generate` is refused |
| P08.3 | Text → playable audio file under `/downloads`; same cap / audit rules as image |
| P08.4 | Tester sees the image in chat and can play the clip without leaving Preview |
| P08.5 | Same zip boots on a machine without NVIDIA (CPU backend) and uses CUDA when present; Settings gpu/cpu take effect after modeld restart (no reinstall); auto demotes/promotes under load with hysteresis; pin overrides auto |
| P08.6 | CUDA-capable host can download media packs; CPU-only host still boots without them |
| P08.7 | Hygiene PR(s) land with no intentional behavior change; workspace tests + p4/p5 still green |
| P08.8 | FEATURES/STATUS/TESTER + version 0.8.0 |
| Regression | `cargo test --workspace`; gates p4/p5 green on CUDA host |
| Packaging | Two tester artefacts (Win / Linux) + complete `latest.json` (`-CpuOnly` = builder-only) |

## Out of scope

Video generation, cloud image APIs as the default path, always-on microphone
/ STT, messaging channels, computer-use, `sandboxed_webview`, E13 compositor,
mid-token device hot-swap without cancel (**that is P09 / E18**), E7 TPM, live HTTP sibling daemon,
E9 / P5.2 multi-GPU, public marketplace, PC cohort close, macOS, bare metal
(E11–E13).

## Build

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda   # tester artefact (GPU+CPU backends)
.\packaging\build-preview.ps1 -SkipModels -CpuOnly       # builder-only, no CUDA toolkit
```

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh   # builder-only
```

## Next

Tag `v0.8.0` only when gates P08.1–P08.8 pass. PC cohort gate remains
independent. **Next: Preview 0.9.0 / E18** — mid-token device migrate without
cancel ([phase-preview-09.md](phase-preview-09.md)). After 0.9: remaining
Horizon B (E7 TPM, live HTTP adapter if a daemon is scheduled, E9 when a
second GPU exists).
