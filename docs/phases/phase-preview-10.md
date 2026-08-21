# Phase P10 — Preview 0.10.0

**Language:** English | [Français](../fr/phases/phase-preview-10.md)

## Goal

Ship **Akasha OS Preview 0.10.0**: remaining **Horizon B** on the host —
**E7 TPM** vault envelope, **E8 live** sibling HTTP bridge daemon,
**E9 / P5.2** multi-GPU code path — plus **Media/UX polish** (document and
harden post-0.9 studio depth). In parallel, an **internal seL4 integration**
channel (`sel4-pv-0.10.0` + CI QEMU gate) that does **not** ship in the
Preview zip or `latest.json`.

Depends on P09 (E18 + E19). Not a new P6 gate number. Not bare metal.
Not PC cohort close. Not PCR sealing / attestation.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon B
remainder. Sequencing: P10.12 early ∥ P10.5–P10.8 ∥ P10.1–P10.4 ∥
P10.9–P10.11 ∥ P10.S* → P10.13–P10.16.

## Versioning (two channels)

| Channel | Tag | Product |
|---------|-----|---------|
| Public Preview | `v0.10.0` + workspace `0.10.0` | Win/Linux tester zips + `latest.json` |
| Internal seL4 | `sel4-pv-0.10.0` (never `v*`) | CI artefacts only (loader + log) |

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P10.1 | E7 TPM | `MasterBackend::Tpm` via Win Platform Crypto `NCrypt` seal (`TPM2` blob); Linux keyring/file until tpm2 wired; legacy `TPM1` migrated | done |
| P10.2 | E7 TPM | Prefer TPM → keyring → file; env overrides; `master.backend=tpm` | done |
| P10.3 | E7 TPM | One-shot migrate keyring/file → TPM when hardware present; `vault.enc` unchanged | done |
| P10.4 | E7 TPM | Tests skip-if-no-TPM; TESTER/Settings backend visibility; agents still denied `secrets.get` | done |
| P10.5 | E8 live | Crate `aos-bridge` → `aos-bridged`: loopback HTTP `/v1`, JSON↔CBOR via bus | done |
| P10.6 | E8 live | Health + core `mem.*` + `secrets.list`; `secrets.get`/`set` service-style only (403 agents) | done |
| P10.7 | E8 live | Per-request `X-Aos-From` → `Intent.from` | done |
| P10.8 | E8 live | Opt-in launch; smoke script; sibling-bridge.md status update | done |
| P10.9 | E9 | `LoadOptions` + llama `tensor_split` / device list (layer pipeline) | done |
| P10.10 | E9 | Multi-GPU `HardwareProfile` + Placement Manager partition | done |
| P10.11 | E9 | `aos-gate-p5` real device count; pass/fail if ≥2 GPUs, skip if 1 | done |
| P10.12 | Media | UI hygiene: `_f32` strokes; wire `metrics_ram` / `metrics_disk` / `models_media_packs` | done |
| P10.13 | Media | FEATURES / TESTER / site: upscale, composition, expert; Wan/LTX experimental | done |
| P10.14 | Media | No first-class img2img/inpaint; no product video surface | done |
| P10.S1 | seL4 | Document public vs `sel4-pv-*` versioning | done |
| P10.S2 | seL4 | CI workflow QEMU gate → `AOS_GATE_VM_PASS` | done |
| P10.S3 | seL4 | Tag `sel4-pv-0.10.0` (independent of `v0.10.0`) | done |
| P10.15 | Docs | phase-preview-10 + STATUS/FEATURES/TESTER/roadmap | done |
| P10.16 | Ship | Version `0.10.0`; tag `v0.10.0` when host gates green | done |

## Exit gates

| Gate | Criterion |
|------|-----------|
| E7 | `master.backend=tpm` on TPM hosts; fallback otherwise; restart survives; agent `secrets.get` denied |
| E8 | Opt-in `aos-bridged`; health + `mem.context` OK; agent secrets → 403; non-loopback bind refused |
| E9 | Code path + skip on 1 GPU; on ≥2 GPUs, layer-split load + stream tokens |
| Media | `aos-ui-egui` clean of targeted float/dead_code warnings; TESTER upscale + composition |
| seL4 | CI or local run shows `AOS_GATE_VM_PASS`; `sel4-pv-0.10.0` does not touch `latest.json` |
| Regression | `cargo test --workspace`; gates p4/p5 |
| Packaging | Two zips + `latest.json` for `v0.10.0` only |

## Out of scope

PC cohort close, macOS, winget/apt, PCR vault / attestation, sibling binary
merge, assistant-as-module, img2img/inpaint first-class, product video, STT /
24/7 mic, E12 / E13, PV.4+, P5.3 AccelDevice, public marketplace, inventing P6.

## Next

After 0.10: PC cohort close when testers ready; Horizon C / PV.4+ when
scheduled; E9 hard-green only after a documented 2-GPU run.
