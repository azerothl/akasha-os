# Project status

**Language:** English | [Français](fr/STATUS.md)

Summary of delivered phases. Detail: [development-plan.md](development-plan.md),
[phases/](phases/). Shipped Preview surface: [FEATURES.md](FEATURES.md).

**Headline:** P0 ✅ / P1 ✅ / P2 ✅ / P3 ✅ / P4 ✅ / PV.1–PV.3 ✅ / P5.1 ✅ / PC 🚧

**Preview:** 0.13.0 (26/08/2026) — reference image, inpaint, short video, chat documents (#52–#55). Not a bootable OS. Cohort gate
still open. **Next:** PC cohort close; Horizon C / PV.4+ when scheduled; E9
hard-green after a documented 2-GPU run.

## P13 — Preview 0.13.0 (create + chat documents) — done

| # | Item | Status |
|---|------|--------|
| P13.1 | Reference image in generation (#52) | done |
| P13.2 | Inpaint / Fix a region (#53) | done |
| P13.3 | Short video (#54) | done |
| P13.4 | Chat document attach (#55) | done |
| P13.5 | Docs EN+FR + website 0.13.0 | done |

## P12 — Preview 0.12.0 (vision + macOS ship) — done

| # | Item | Status |
|---|------|--------|
| P12.1 | Vision infer path + mmproj catalog sidecars | done |
| P12.2 | Chat UI image attach + non-vision gate | done |
| P12.3 | Canvas `set_style` + tool orchestration fixes | done |
| P12.4 | macOS Apple Silicon Preview zip + mill download | done |
| P12.5 | Docs EN+FR + website 0.12.0 | done |
| P12.6 | 0.12.1 patch — Gemma 4 Jinja chat-template fallback (#49) | done |

## P11 — Preview 0.11.0 (E20 local decode) — done

| # | Item | Status |
|---|------|--------|
| P11.1 | KV Q8_0 / F16 via `LoadOptions` + Placement factor | done |
| P11.2 | Prefix cache (`memory_seq_rm` + warm `llama_state_*`) + E18 restore | done |
| P11.3 | Prompt-lookup speculative on C1; batch N>1 unchanged | done |
| P11.4 | `draft_accept` / `prefix_hit` metrics + UI | done |
| P11.5 | Docs EN+FR | done |

Detail: [phases/phase-preview-11.md](phases/phase-preview-11.md).

## 0.10.1 — Bridge parity + product polish — done

| # | Item | Status |
|---|------|--------|
| Bridge | Full live `mem.*` routes on `aos-bridged` + packaged in zip | done |
| Updates | Opt-in auto-download; pending banner; apply on next launch | done |
| E15 | `pie` + `scatter` host widgets (no webview) | done |
| Media | img2img via closed `init_image` / `strength` options | done |

## P10 — Preview 0.10.0 (E7 TPM + E8 live + E9 + Media + seL4 internal) — done

| # | Evolution | Status |
|---|-----------|--------|
| P10.1–P10.4 | E7 TPM master-key envelope | done |
| P10.5–P10.8 | E8 `aos-bridged` minimal live adapter | done |
| P10.9–P10.11 | E9 multi-GPU path + honest 1-GPU skip | done |
| P10.12–P10.14 | Media/UX polish (studio depth + hygiene) | done |
| P10.S1–P10.S3 | Internal seL4 CI / `sel4-pv-0.10.0` | done |
| P10.15–P10.16 | Docs / packaging / version 0.10.0 | done |

Detail: [phases/phase-preview-10.md](phases/phase-preview-10.md).

## P09 — Preview 0.9.0 (E18 + E19) — done

| # | Evolution | Status |
|---|-----------|--------|
| P09.1 | E18 mid-token CPU ↔ GPU migrate (stream continues) | done |
| P09.2 | E18 UI/`auto` uses migrate; 0.8 cancel+restart is fallback | done |
| P09.3 | E19 closed option schema on `media.*` + Settings (sd.cpp / Piper allowlist) | done |
| P09.4 | E19 extra optional packs (Flux2, Ideogram4, extra Piper voices) | done |
| P09.5 | E19 Models / Settings surface; `/image` and TTS honor the choice | done |
| P09.8 | E19 Image studio page + chat control; in-chat TTS options card | done |
| P09.7 | Finish P08.8 leftovers (split remaining host modules; leftover i18n; chat role key) | done |
| P09.6 | Docs / packaging / version 0.9.0 | done |

Detail: [phases/phase-preview-09.md](phases/phase-preview-09.md).

## P08 — Preview 0.8.0 (E16 + E17 + E15 widgets + Providers) — done

| # | Evolution | Status |
|---|-----------|--------|
| P08.1 | E16 media registry + Placement Manager shards | done |
| P08.2 | E16 `media.image.generate` (PNG under `/downloads`) | done |
| P08.3 | E16 `media.audio.generate` (TTS) | done |
| P08.4 | E16 chat surface (`image`/`audio` kinds in P08.11) | done |
| P08.5 | E17 unified artefact + UI / load-based device policy | done |
| P08.6 | E16 optional media packs (Download also fetches sd.cpp / piper) | done |
| P08.7 | F-MOD-01 module uninstall (UI, revoke caps, drop E15 tab) | done |
| P08.8 | Cleanup + refactor (Preview host; no behavior change) | done |
| P08.9 | One-line install per OS (download + sha256 + overlay) | done |
| P08.10 | Docs / packaging / version 0.8.0 | done |
| P08.11 | E15 widget vocabulary (select/radio/checkbox/textarea/bar_chart/image/audio + typed forms) | done |
| P08.12 | F-MDL-04 Providers tab (OpenAI-compat cloud + local servers) | done |

Detail: [phases/phase-preview-08.md](phases/phase-preview-08.md).

## P07 — Preview 0.7.0 (E15) — done

| # | Evolution | Status |
|---|-----------|--------|
| P07.1 | E15 closed widget schema (fail-closed unknown kinds) | done |
| P07.2 | E15 generic egui tab host for `declarative_ui` modules | done |
| P07.3 | E15 bind tools → table/chart/form; actions → `tool.invoke` | done |
| P07.4 | E15 scaffold/package writes a real widget tree | done |
| P07.5 | Docs / packaging / version 0.7.0 | done |

Detail: [phases/phase-preview-07.md](phases/phase-preview-07.md).

## P06 — Preview 0.6.0 (E8 / E7-keyring / E10)

| # | Evolution | Status |
|---|-----------|--------|
| P06.1 | E8 JSON Schema export `mem.*` / `secrets.*` | done |
| P06.2 | E8 HTTP↔bus contract (no live daemon) | done |
| P06.3 | E7 OS keyring wrap of vault master key | done |
| P06.4 | E10 local signed module catalogue | done |
| P06.5 | Stretch: chat Stop + clipboard | done |
| P06.6 | Docs / packaging / version 0.6.0 | done |

Detail: [phases/phase-preview-06.md](phases/phase-preview-06.md).

## P05 — Preview 0.5.0 (E14)

| # | Evolution | Status |
|---|-----------|--------|
| P05.1 | E14 `mem.extract` + secret filter | done |
| P05.2 | E14 persist + auto_link + audit | done |
| P05.3 | E14 Settings opt-in + toast + badge | done |
| P05.4 | E14 tests + TESTER 7d | done |
| P05.5 | Docs / packaging / version 0.5.0 | done |

Detail: [phases/phase-preview-05.md](phases/phase-preview-05.md).

## P04 — Preview 0.4.0 (E6 / E7 / E10-lite)

| # | Evolution | Status |
|---|-----------|--------|
| P04.1 | E6 typed memory graph | done |
| P04.2 | E6 bootstrap + Memory UI | done |
| P04.3 | E7-lite secrets vault | done |
| P04.4 | E10-lite cap review + MCP example | done |
| P04.5 | Docs / packaging / sibling-bridge | done |

Detail: [phases/phase-preview-04.md](phases/phase-preview-04.md).

## P03 — Preview 0.3.0 (E1–E5)

| # | Evolution | Status |
|---|-----------|--------|
| P03.1 | E5 metrics UI (TTFT / tok/s / VRAM) | done |
| P03.2 | E4 caps UI (`cap.list` / revoke) | done |
| P03.3 | E1 CPU-only path + packaging | done |
| P03.4 | E2 agent scheduler | done |
| P03.5 | E3 tasks dual-surface module | done |

Detail: [phases/phase-preview-03.md](phases/phase-preview-03.md).

## P0 — Simulator (validated)

| Deliverable | Crate | Content |
|-------------|-------|---------|
| P0.1 | `crates/aos-placement` | Placement Manager simulator (§3.5) |
| P0.2 | `crates/aos-caps` | Logical capability model (§2.3), 20 security tests |
| P0.3 | `crates/aos-registry` | YAML catalog + simulated backends |
| P0.4 | `crates/aos-sim` | Six §17.2 scenarios + llama.cpp cross-check (`xval`) |

## P1 — Real model subsystem (gate 6/6)

| Deliverable | Crate | Content |
|-------------|-------|---------|
| P1.1–P1.3 | `aos-llama`, `aos-model` | llama.cpp FFI (CUDA), placement, scheduler, metrics |
| P1.4 | `aos-agent` | Agent runtime: isolated workers, caps, cognitive state |
| P1.5 | `aos-ipc` | Semantic IPC bus v1 (CBOR, typed intents, streams) |
| P1.6 | `aos-ui` | TUI chat + resource dashboard |

P1 gate (RTX 4080S): warm TTFT **18 ms**; 32B Q6 offload ~2 tok/s.

## P2 — WASM modules + memory + audit (gate 6/6)

| Deliverable | Location | Content |
|-------------|----------|---------|
| P2.1–P2.2 | `aos-platform` (`module_rt`) | wasmtime sandbox, cap injection |
| P2.3 | `memory` | Working + episodic embeddings |
| P2.4 | `storage` | Versioned FS, undo, classification |
| P2.5 | `audit` | Append-only hashed journal |
| P2.6 | `modules/notes` + SDK | Dual-surface notes module |

## P3 — Remote backends + security (gate 4/4)

| Deliverable | Content |
|-------------|---------|
| P3.1 | OpenAI-compatible remote backend |
| P3.2 | Declarative policy engine |
| P3.3 | Egress deny-by-default |
| P3.4 | Blocking confirmation (fail-closed) |
| P3.5 | Trust manager + `cap.request` |
| P3.6 | Supervisor notifications / conflict arbitration |

## P4 — Native caps + isolation (gate 4/4)

| Deliverable | Content |
|-------------|---------|
| P4.1 | Userspace cap kernel on host ([ADR 0001](../adr/0001-microkernel.md)) |
| P4.2 | `aos-capkd` mint/derive/grant/revoke/check |
| P4.3 | Native caps in IPC envelope |
| P4.4 | Isolated daemons + autonomous auditd |
| P4.5–P4.6 | Host shells + offline demo gate |

## PV — seL4 VM track

| Deliverable | Content |
|-------------|---------|
| PV.1–PV.3 | Microkit QEMU aarch64, intent bus, CapStore in guest |

See [phases/phase-vm-sel4.md](phases/phase-vm-sel4.md), `.\demo\run-sel4-vm.ps1`.

## P5.1 — Continuous batching (host gate)

`generate_batch` / `n_seq_max=8`. Multi-GPU not required on single-GPU hosts.
Detail: [phases/phase-p5.md](phases/phase-p5.md).

## PC — Preview cohort (installable host)

| Deliverable | Status |
|-------------|--------|
| PC.1 Session supervisor | done |
| PC.2 Packaging Win/Linux + CI | done |
| PC.3 egui tester UI + tutorial | done |
| PC.4 Feedback → GitHub issues | done |
| PC.5 INSTALL / TESTER / FIRST-RUN | done |
| PC.6–PC.9 Sessions, memory, search, files | done |
| PC.10 Non-destructive Release updates | done |
| PC.11 Agent transparency (timeline, sources, steer) | done |
| PC.12 Settings + persisted preferences | done |
| PC.13 Multi-engine search + `web.browse` | done |
| PC.14 Memory-first bootstrap + Qwen think strip | done |
| Hardware-aware first-run model setup | done |
| Public site (EN/FR, Split-Flap Board) | done |
| Notes package resync on boot | done |
| In-app Troubleshoot report | done |

Cohort gate (3 Win + 1 Linux testers, no toolchain) remains **open**.

Detail: [phases/phase-pc.md](phases/phase-pc.md), [FEATURES.md](FEATURES.md).

## Dev commands

```powershell
cargo test --workspace
.\demo\run-demo.ps1 -Gate p4
.\demo\run-demo.ps1 -Gate p5
.\packaging\build-preview.ps1 -SkipModels
```
