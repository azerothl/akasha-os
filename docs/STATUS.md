# Project status

**Language:** English | [Français](fr/STATUS.md)

Summary of delivered phases. Detail: [development-plan.md](development-plan.md),
[phases/](phases/).

**Headline:** P0 ✅ / P1 ✅ / P2 ✅ / P3 ✅ / P4 ✅ / PV.1–PV.3 ✅ / P5.1 ✅ / PC 🚧

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

Detail: [phases/phase-pc.md](phases/phase-pc.md).

## Dev commands

```powershell
cargo test --workspace
.\demo\run-demo.ps1 -Gate p4
.\demo\run-demo.ps1 -Gate p5
.\packaging\build-preview.ps1 -SkipModels
```
