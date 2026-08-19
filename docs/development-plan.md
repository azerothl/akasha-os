# Development plan by phases — Agent OS

**Language:** English | [Français](fr/plan-developpement-phases.md)

> Version: 1.5  
> Date: 19/08/2026  
> Status: reference plan  
> References: `docs/functional-specs.md`, `docs/technical-specs.md`, `docs/vision.md`, `docs/FEATURES.md`, `docs/evolution-roadmap.md`

---

## 0. Overview

The development of Agent OS has two layers that must not be collapsed:

1. **Foundation phases (P0–P5 + PV + PC)** — prove placement, caps, WASM,
   security, host isolation, seL4 scaffolding, and the installable Preview
   cohort. Bare metal = after PV (ADR 0001).
2. **Preview increment phases (P03–P09)** — ship host-app releases **without
   inventing a P6 gate** while the PC cohort is still open. Numbering is
   `P0n` / Preview `0.n.0` (E* from [evolution-roadmap.md](evolution-roadmap.md)).

P0–P5 prove and polish the system **on the host**; **PV** is the **seL4 kernel
scaffolding** (QEMU VM, without GPU); **PC** is the **distributable Preview**
for a tester cohort (Win/Linux installer, not seL4). The
`docs/technical-specs.md` §1.3 strategy remains: prove agentic algorithms in
userspace before committing to the microkernel port.


| Phase       | Foundation                            | Central objective                                                                             | Indicative duration | Status      |
| ----------- | ------------------------------------- | --------------------------------------------------------------------------------------------- | ------------------- | ----------- |
| **P0**      | Standalone Rust simulator / prototype | Validate the **RAM/GPU/disk placement algorithm** and the capability model                    | ~6–8 weeks          | done        |
| **P1**      | Linux host (isolated processes)       | Complete **Model Subsystem**: local inference, real offload, multi-process agents, minimal UI | ~10–14 weeks        | done        |
| **P2**      | Linux host                            | **WASM modules + memory + audit/undo**                                                        | ~8–10 weeks         | done        |
| **P3**      | Linux host                            | **Remote backends, privacy routing, complete security**                                       | ~6–8 weeks          | done        |
| **P4**      | Host (userspace caps)                 | **Microkernel semantics** (`aos-capkd`, process isolation) — seL4 deferred (GPU)              | ~12–16 weeks        | done        |
| **PV**      | QEMU + seL4 (without GPU)             | **Real kernel port**: Microkit PDs, seL4 IPC, CPU-only replay of the P4 gate                  | ~8–10 weeks         | PV.1–PV.3 ✅ |
| **PC**      | Win/Linux host + NVIDIA               | Installable **0.1 Preview**: session, egui, cohort feedback                                   | ~2–4 weeks          | 🚧 cohort   |
| **P5**      | GPU host / polish                     | **First-class GPU/NPU**, multi-GPU, polish (parallel with PV/PC)                              | ~10–12 weeks        | P5.1 ✅      |
| **P03**     | Win/Linux Preview host                | Preview **0.3.0** — E1–E5 (CPU path, scheduler, tasks, caps UI, metrics)                      | shipped             | done        |
| **P04**     | Win/Linux Preview host                | Preview **0.4.0** — E6 / E7-lite / E10-lite (memory graph, vault, cap review)                 | shipped             | done        |
| **P05**     | Win/Linux Preview host                | Preview **0.5.0** — E14 (auto-remember from chat)                                             | shipped             | done        |
| **P06**     | Win/Linux Preview host                | Preview **0.6.0** — E8 / E7-keyring / E10 (bridge schemas, OS keyring, catalogue)             | shipped             | done        |
| **P07**     | Win/Linux Preview host                | Preview **0.7.0** — E15 (host-rendered declarative module UI)                                 | shipped             | done        |
| **P08**     | Win/Linux Preview host                | Preview **0.8.0** — E16 image+audio + E17 unified CPU/GPU + module uninstall + E15 widgets + Providers | shipped             | done        |
| **P09**     | Win/Linux Preview host                | Preview **0.9.0** — E18 mid-token migrate + E19 media models/options + chat media plugins    | ~4–6 weeks          | done        |


**Indicative total** (foundation): ~60–80 weeks in a naïve sequence;
**PV ∥ P5 ∥ PC** brings the critical path closer. Preview increments
(P03–P09) run on the already-shipped host stack. These durations are rough
orders of magnitude—each exit gate takes precedence over the schedule.

---



## Cross-cutting principle: validation gates

Each phase ends with an **exit gate**: an executable demonstration and a
measurable criterion. **The next phase does not start until the gate has
passed.** This prevents technical debt from accumulating in the lower layers.


| Gate        | Measurable criterion (exit gate)                                                                                                                                                 |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Gate P0** | Correct simulation of the 6 placement scenarios (§17.2 `specs-tech`) with estimated tok/s consistent with real measurements on llama.cpp                                         |
| **Gate P1** | Boot Linux demo → conversational assistant (embedded model) + successful inference on a model whose size is > VRAM (active offload), TTFT < 2s warm                              |
| **Gate P2** | Installation of a dual-surface module used by an agent (tool) and a human (UI), visible audit trail, effective undo of a file action                                             |
| **Gate P3** | Automatic local→remote switch according to privacy policy, `local_only` mode verifiable through egress monitoring, blocking confirmation for a sensitive action                  |
| **Gate P4** | Essential services isolated + native userspace caps (`aos-capkd`) on the host; kill Audit without affecting Model                                                                |
| **Gate PV** | seL4/Microkit boot under QEMU `virt` (without GPU); `cap.`* intents through the PD bus; immediate revocation; stop Audit without killing CapKernel                               |
| **Gate PC** | 3 Windows testers + 1 Linux tester install Preview without a toolchain; `TESTER.md` protocol; ≥1 actionable `feedback.submit`                                                    |
| **Gate P5**  | Continuous batching for multiple agents with < 20% degradation over 8 simultaneous streams, functional multi-GPU pipeline, aarch64 port validated on at least one target machine |
| **Gate P03** | Metrics + caps UI + CPU-only boot + scheduler fire + tasks dual-surface — [phase-preview-03.md](phases/phase-preview-03.md) |
| **Gate P04** | Typed memory graph + vault + module cap review — [phase-preview-04.md](phases/phase-preview-04.md) |
| **Gate P05** | Opt-in chat extract → `mem.user.remember` (secrets filtered) — [phase-preview-05.md](phases/phase-preview-05.md) |
| **Gate P06** | Bridge schemas + OS keyring + signed local catalogue — [phase-preview-06.md](phases/phase-preview-06.md) |
| **Gate P07** | Closed `declarative_ui` schema; generic egui tab bound to module tools — [phase-preview-07.md](phases/phase-preview-07.md) |
| **Gate P08** | Local image + TTS under caps; unified CPU/GPU artefact; module uninstall; E15 widget pack; Providers tab; one-line install per OS; cleanup/refactor — [phase-preview-08.md](phases/phase-preview-08.md) |
| **Gate P09** | Mid-token CPU ↔ GPU migrate without cancelling the live stream; failed migrate falls back to 0.8 cancel+restart (audited); closed image/TTS option schema + extra packs (Flux2, Ideogram4, extra Piper voices); Image studio page + in-chat TTS options card; leftover P08.8 host hygiene — [phase-preview-09.md](phases/phase-preview-09.md) |


---



## Phase P0 — Algorithm proof (simulator)



### Objective

Validate, without writing an OS yet, that the RAM/GPU/disk placement algorithm
produces realistic plans and that the capability model is consistent.
**Output: a standalone Rust simulator.**

### Deliverables


| #    | Deliverable                 | Description                                                                                                                                                              |
| ---- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| P0.1 | Placement Manager simulator | Rust program that accepts a model (size, number of layers), a hardware configuration (VRAM/RAM/disk), and a profile, and produces a placement plan + tok/s/TTFT estimate |
| P0.2 | Logical capability model    | Rust types `Cap`, `Rights`, and `mint/derive/grant/revoke` operations with security tests (attenuation, cascading revocation)                                            |
| P0.3 | Registry + fake backends    | YAML model catalog, mocked backends that simulate response times                                                                                                         |
| P0.4 | Scenario test bench         | The 6 scenarios from `docs/technical-specs.md` §17.2 automated                                                                                                           |




### Technical dependencies

- Stable Rust, `serde`, `criterion` (bench)
- No OS-specific dependency (everything is standalone)



### Exit gates (Gate P0)

- [ ] The 6 placement scenarios pass with a tok/s estimation error < 30% vs. real llama.cpp measurements (cross-validation)
- [ ] Capability model: 100% of security tests pass (strict attenuation, tree revocation)
- [ ] Placement algorithm documentation published in `adr/0002-model-placement.md`



### Specific risks


| Risk                                             | Mitigation                                                                                   |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------- |
| The tok/s estimate diverges too far from reality | Empirical calibration with llama.cpp from P0 (real measurements to calibrate the cost model) |


---



## Phase P1 — Real Model Subsystem (Linux userspace)



### Objective

Replace mocks with a real implementation: local inference with llama.cpp,
active RAM/disk offload, multi-process agents with logical caps, and a minimal
UI. **Output: a usable Linux demonstrator, without a real dedicated kernel
yet.**

### Deliverables


| #    | Deliverable                    | Description                                                                                        |
| ---- | ------------------------------ | -------------------------------------------------------------------------------------------------- |
| P1.1 | Model Subsystem v1             | Registry, Backend Manager (llama.cpp through FFI), Tokenizer Service, Metrics Exporter             |
| P1.2 | Real Placement Manager         | Replace the simulator with an implementation that actually controls VRAM/RAM/mmap allocations      |
| P1.3 | Inference Scheduler v1         | Priority queues, simple batching, cancellation                                                     |
| P1.4 | Agent Runtime v1               | Isolated process per agent (not kernel caps yet), complete lifecycle, serializable Cognitive State |
| P1.5 | Semantic IPC Bus v1            | Typed message bus (CBOR) between processes                                                         |
| P1.6 | Minimal UI                     | Conversational shell + resource dashboard (VRAM/RAM/disk/agents)                                   |
| P1.7 | Best-effort aarch64 validation | Compilation and execution on Apple Silicon or ARM64 Linux in parallel (non-blocking)               |




### Technical dependencies

- llama.cpp (FFI through `llama-cpp-sys` or equivalent)
- `tokio` (async), `serde`, `ciborium` (CBOR)
- UI: choice to be finalized in P1 (egui/iced/tauri, see decision §14 `docs/technical-specs.md`)



### Exit gates (Gate P1)

- [x] Linux demo boot → conversational assistant on `embedded-instruct` (TTFT < 2s warm) — **validated on the Windows development host (measured warm TTFT: 21 ms); the Linux run remains to be replayed (WSL2 is present on the host, Rust toolchain still needs to be installed there)**
- [x] Successful inference on a 32B Q6 model with only 8 GB of VRAM (active RAM+disk offload, visible on the dashboard) — **measured: 6.12 GiB VRAM + 19.9 GiB RAM (11/53 layers), 1.72 tok/s; DISK tier covered by mmap lazy paging on this host (model < RAM), constrained streaming with a model > RAM remains to be demonstrated**
- [x] Two concurrent agents run in parallel without mutually crashing — **2 isolated worker processes, simultaneous production verified by** `aos-gate-p1`
- [x] Killing an agent has no impact on the Model Subsystem or UI — **verified by** `aos-gate-p1` **(**`taskkill /F` **+ post-kill inference OK)**

> P1 status (12/08/2026): gate passed on the development host (Windows +
> RTX 4080S + CUDA) with `aos-gate-p1` — 6/6 executable criteria green.
> Documented gaps: Windows host instead of Linux (cross-platform code, Linux
> run to be replayed), scheduler = priority queues + cancellation (mature
> continuous batching = P5), UI = ratatui TUI (formal GUI decision deferred,
> see `adr/0003-ui-framework.md`), agent pause = abandon + regeneration
> (resume to the exact token = P5).



### Specific risks


| Risk                                                 | Mitigation                                                                                                                                                           |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Disk offload performance is too low to be usable     | Aggressive `memory-saver` profile + DMA prefetch + recommended NVMe (see `docs/technical-specs.md` §18); cross-validation with the P0 simulator to detect this early |
| Poor UI framework choice becomes difficult to change | Prototype both egui AND iced for one week, decide in ADR `0003` before continuing                                                                                    |


---



## Phase P2 — WASM modules + memory + audit/undo



### Objective

Make the system **extensible** and **auditable**: WASM sandbox, episodic
memory, audit trail, filesystem undo. **Output: the system becomes a platform
on which modules can be installed.**

### Deliverables


| #    | Deliverable                      | Description                                                                                 |
| ---- | -------------------------------- | ------------------------------------------------------------------------------------------- |
| P2.1 | Module Registry + Module Runtime | `.aospkg` format, dual-surface manifest, sandboxed WASM loading (wasmtime)                  |
| P2.2 | Logical caps v2                  | Inject capabilities into WASM modules, schema introspection                                 |
| P2.3 | Memory Subsystem                 | Working memory per agent + vector store (hnswlib-rs) + `mem.*` API                          |
| P2.4 | Storage Subsystem v1             | Filesystem with snapshots (btrfs/ZFS or logical copy-on-write fallback), transactions, undo |
| P2.5 | Audit trail                      | Signed append-only journal, accessible through the UI                                       |
| P2.6 | Reference module                 | A complete “notes” module demonstrating the dual surface (agent tool + human UI)            |




### Technical dependencies

- `wasmtime` (WASM runtime)
- `hnswlib-rs` or `usearch` (vector index)
- Filesystem: btrfs/ZFS if available, otherwise a logical userspace snapshot implementation



### Exit gates (Gate P2)

- [x] The “notes” module is installed, used by an agent (creating a note through a tool), and used by a human (UI) — **agent: worker** `TOOL:` **convention →** `module.invoke`**; human:** `module.invoke` **as** `human:ui` **from the TUI**
- [x] Audit trail shows the complete chain: intent → agent → tool → fs — `tool.invoke (agent:…)` **→** `fs.write (module:notes)` **under one** `trace_id`**, HMAC integrity verified**
- [x] Undo of an agent-created file restores the previous state — **logical COW (versions), “did not exist before”**
- [x] A module attempting to access a file without a capability is refused and audited — **agent without** `tool.invoke:notes` **→ refusal + audit event**

> P2 status (12/08/2026): gate passed on the development host with
> `aos-gate-p2` — 6/6 executable criteria green. Documented gaps: exact
> brute-force vector index (swap to ANN usearch/hnswlib-rs through the
> `VectorIndex` trait), filesystem snapshots = logical userspace manifests
> (not btrfs/ZFS on the host), module package = `.aospkg/` directory (signed
> archive later), capability review at installation auto-approved in the demo
> (review UI in P3).
>
> User feedback post-gate (12/08/2026), integrated immediately: conversation
> panel scrolling (PageUp/PageDown + auto-follow), `/commands` (list) and
> `/help` (OS status: services, agents, memory, models, audit), “Agent OS
> knowledge” system prompt injected into the assistant and agents
> (`aos_proto::SYSTEM_ASSISTANT_PROMPT`). Remaining for P5 (P5.4 advanced UI):
> history search, graphical transparency panel, full keyboard navigation.



### Specific risks


| Risk                                                     | Mitigation                                                                                                  |
| -------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| WASM sandbox performance is insufficient for heavy tools | Provide a “native privileged” mode (outside WASM) only for critical system modules, with strict code review |


---



## Phase P3 — Remote backends + complete security



### Objective

Add **remote backends**, **privacy-aware routing**, and **complete security**
(egress, blocking confirmation). **Output: the system is functionally complete
as a v1 userspace system.**

### Deliverables


| #    | Deliverable                      | Description                                                                                         |
| ---- | -------------------------------- | --------------------------------------------------------------------------------------------------- |
| P3.1 | OpenAI-compatible remote backend | HTTP/SSE client, authentication through secret caps                                                 |
| P3.2 | Local/remote routing             | Policy engine applying `local_only`/`remote_only`/`balanced`, sensitivity classification (F-FS-05)  |
| P3.3 | Network Egress Control           | `net.connect` capabilities, deny by default, strict offline mode                                    |
| P3.4 | Blocking confirmation            | `require_confirmation` effect in the Policy Engine, `pending_confirmation` IPC flow, Control bar UI |
| P3.5 | Trust Manager v1                 | Trust score, levels, user governance                                                                |
| P3.6 | Supervisor agent v1              | Notification aggregation + resource-conflict arbitration                                            |




### Technical dependencies

- HTTP client: `reqwest` (Rust)
- Policy engine: simple declarative language (YAML → Rust rules engine)



### Exit gates (Gate P3)

- [x] An intent referencing `secret` data is always routed locally, even if a remote backend is configured — **verified by** `aos-gate-p3`**: 0 hits on the mock SSE,** `policy.deny (deny_remote_secret)` **audited, response served by the local model**
- [x] `local_only` mode: no outgoing packet to model backends detected (verified by egress monitoring) — `net.egress_log` **empty for the backend + 0 mock hits; all egress goes through the Backend Manager (single userspace control point)**
- [x] An `fs.delete` action triggers a blocking confirmation; timeout → audited refusal — **3 s blocking confirmation (gate config), fail-closed audited refusal (**`confirmation.resolved approved=false`**), file intact**
- [x] An agent with a high trust score obtains an additional capability without confirmation; an agent with a low score is refused — **tiered Trust Manager: high → immediate** `Granted`**, low →** `Denied`

> P3 status (12/08/2026): gate passed on the development host with
> `aos-gate-p3` — 4/4 executable criteria green. The remote backend is tested
> against a **local OpenAI-compatible mock SSE** (no real API key required);
> the reqwest/SSE client is complete. Minimal supervisor v1 (deduplicated
> notifications + filesystem transaction-conflict arbitration). Gaps:
> secret encryption = local file in v1 (hardware/TPM envelope deferred),
> capability review at installation still auto-approved in the demo.



### Specific risks


| Risk                     | Mitigation                                                                                          |
| ------------------------ | --------------------------------------------------------------------------------------------------- |
| Policy engine complexity | Start with only 3 effects (`allow`/`deny`/`require_confirmation`), with deliberately limited syntax |


---



## Phase P4 — Microkernel port (seL4 / Redox)



### Objective

This is the **turning point**: port the validated userspace services to a real
capability-based microkernel. **Output: Agent OS no longer depends on Linux.**

### Deliverables


| #    | Deliverable                   | Description                                                                            |
| ---- | ----------------------------- | -------------------------------------------------------------------------------------- |
| P4.1 | Microkernel choice + bring-up | seL4 (recommended for formal verification) or Redox; minimal boot, init, basic drivers |
| P4.2 | Native caps                   | Replace the P1–P3 logical caps with kernel capabilities                                |
| P4.3 | Native semantic IPC           | Port the Semantic IPC Bus to the microkernel's IPC primitives                          |
| P4.4 | Service port                  | Model Subsystem, Agent Runtime, Storage, Policy, Audit — each as a server process      |
| P4.5 | UI on microkernel             | Minimal compositor + shell ports (may initially be partial)                            |
| P4.6 | Complete offline boot         | §10 boot sequence on the microkernel, embedded models loaded                           |




### Technical dependencies

- seL4 (with `sel4-sys` / Rust bindings) or Redox OS
- Drivers: GPU/NPU and NVMe in microkernel userspace (the greatest risk)



### Exit gates (Gate P4)

- [x] All essential services (Model, Agent, Storage, Policy, Audit) run as isolated processes, alongside the caps kernel (`aos-capkd`) — **verified by** `aos-gate-p4` **through** `bus.lookup`
- [x] Killing a non-critical service (Audit) has no impact on the Model Subsystem or UI — `aos-auditd` **killed;** `model.list` **+ inference OK**
- [x] A capability revoked at kernel level is immediately invalid for all processes — **mint →** `fs.write`**/**`fs.read` **through platformd → revoke →** `cap.check` **and** `fs.read` **refused without delay**
- [x] Offline boot → functional conversational assistant (same level as Gate P1) — **local inference without a network**

> P4 status (12/08/2026): gate passed on the development host with
> `aos-gate-p4` — 4/4 executable criteria green. **ADR 0001 decision**:
> userspace caps kernel (`aos-capkd`) + process isolation on the host; the
> seL4/Redox port (GPU drivers) is deferred. Semantic IPC = same bus, native
> caps `cap://kernel/<id>` in the envelope. UI = TUI/egui on the host
> (microkernel compositor = P5). Worker-agent caps remain logical (P1);
> filesystem access is judged by the kernel as soon as a kernel cap is
> presented. Hardware secret envelope (TPM) deferred.



### Specific risks


| Risk                                                   | Mitigation                                                                                     |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| GPU/NPU drivers on the microkernel are too complex     | Use a lightweight hypervisor or virtio gateway (device passthrough) in P4, native driver in P5 |
| seL4 complexity (formal verification = learning curve) | Train the team + start with the least critical services (Audit) to build expertise             |


> **Structural decision (P4, ADR 0001)**: seL4 bring-up + GPU is too costly on
> the Windows host **for P4**, not an abandonment of the target. P4 v1 =
> userspace caps kernel (`aos-capkd`) on the host. **Product target: a machine
> that boots Agent OS (seL4), with no other OS.** Path: host (GPU) and QEMU
> seL4 VM without GPU in parallel, then bare metal (`AccelDevice` native,
> P5.3). No GPU passthrough from Windows.

> **VM track**: extracted into **phase PV** (below)—it is no longer a P4 gap;
> it is the kernel port.

---



## Phase PV — seL4 VM track (kernel scaffolding)



### Objective

Port the semantics validated in P4 (caps, isolation, IPC) to a **real seL4**,
in a **GPU-less QEMU VM**. **Output: the P4 gate replayed in the guest**,
with the transport contract = seL4 primitives. Bare metal reuses this image
(ADR 0001). **Parallel to P5** (GPU on the host).

### Deliverables


| #    | Deliverable            | Description                                                                   |
| ---- | ---------------------- | ----------------------------------------------------------------------------- |
| PV.1 | Microkit boot          | `qemu_virt_aarch64` image, `capkd` / `bus` / `auditd` / `gate` PDs            |
| PV.2 | Semantic bus           | `bus` PD: lookup + `cap.*` proxy (seL4 PPC, not TCP)                          |
| PV.3 | `no_std` CapStore      | `aos-caps` without `std`; `aos-sel4-capkd` staticlib linked into the capkd PD |
| PV.4 | Bare-metal preparation | Boot documentation (same image, no virtio-CUDA); `AccelDevice` remains P5.3   |




### Technical dependencies

- seL4 Microkit SDK (prebuilt), QEMU `system-aarch64`, WSL Ubuntu on the Windows host
- `libmicrokit` (C glue for the PDs); Rust `no_std` `CapStore` in capkd (PV.3)



### Exit gates (Gate PV)

- [x] seL4 boot under QEMU + immediate revocation + stopping Audit without killing CapKernel — `AOS_GATE_VM_PASS` **(PV.1, 12/08/2026)**
- [x] `cap.*` intents pass through a bus PD (no direct gate→capkd call) — **lookup + PPC proxy, serial** `bus lookup cap.* OK`
- [x] `aos-caps` `no_std`: 100% of P0.2 security tests — **20/20;** `cargo check -p aos-caps --no-default-features`
- [x] One C/Rust ABI contract (`vm/sel4/abi.h` ≡ `aos-sel4-abi`) — `aligne_sur_abi_h` **test**
- [x] `CapStore` runs in the guest (no duplicated C table) — `aos-sel4-capkd` **staticlib, VM gate replayed**

```powershell
.\demo\run-sel4-vm.ps1
```

> PV status (12/08/2026): **PV.1–PV.3 passed** (boot + intent bus +
> `CapStore` in the guest). PV.4 (bare metal) deferred. See
> `docs/phases/phase-vm-sel4.md`.



### Specific risks


| Risk                                      | Mitigation                                                                |
| ----------------------------------------- | ------------------------------------------------------------------------- |
| seL4 toolchain absent from native Windows | Build/run in WSL Ubuntu; Microkit SDK gitignored                          |
| Rust PD port blocked (`sel4-microkit`)    | Link CapStore as a staticlib; use C glue until the Rust runtime is stable |


---



## Phase PC — Cohort Preview (distributable host)



### Objective

Deliver **Agent OS Preview 0.1**: the same host stack (P1–P5),
**installable** by external testers on Windows and Linux x64 + NVIDIA,
**without compiling**. egui UI = main surface; feedback through
`feedback.submit` (local only). **This is not bare metal** (ADR 0001).

### Deliverables


| #         | Deliverable                        | Description                                                        |
| --------- | ---------------------------------- | ------------------------------------------------------------------ |
| PC.1      | `aos-session`                      | Supervisor: AOS_HOME, configs, ordered boot, auditd watchdog, UI   |
| PC.2      | Preview package                    | `bin/` + first-run GGUF download + notes.aospkg; Win/Linux         |
| PC.3      | Cohort egui UI                     | Onboarding, notes, confirm, agents, audit, scenarios, banner       |
| PC.4      | Feedback                           | `feedback.submit` intent → `var/feedback/` + optional GitHub issue |
| PC.5      | Docs                               | `docs/INSTALL.md`, `docs/TESTER.md`, `docs/FEATURES.md`, `packaging/`   |
| PC.6–PC.9 | Sessions / memory / search / files | Persisted chat, `mem.context`, opt-in net, generate                |
| PC.10     | Release updates                    | Non-destructive overlay of `bin/` + `share/`                       |
| PC.11     | Transparency                       | Agent detail timeline, sources, pause / steer / retry              |
| PC.12     | Settings                           | Persisted prefs (language, routing, trust, web engine)             |
| PC.13     | Browse + engines                   | `web.browse`; Brave / DuckDuckGo / Bing                            |
| PC.14     | Agent bootstrap                    | `task.assess` + memory-first recall; Qwen think strip              |




### Exit gates (Gate PC)

- [x] Win + Linux installer/archive without `cargo`
- [x] `docs/TESTER.md` protocol playable from egui
- [x] ≥1 actionable `feedback.submit` response from the pilot cohort

```powershell
.\packaging\build-preview.ps1
# Linux: ./packaging/build-preview.sh
```

> PC status (18/08/2026): Preview **0.8.0** is the current host release
> (E16 media, E17 unified artefact, Providers). PC.1–PC.14 shipped since 0.1; subsequent product
> work is Preview increments P03–P09, not a new PC.n number. Cohort gate
> still open — see `docs/INSTALL.md` and `docs/FEATURES.md`.



### Specific risks


| Risk                              | Mitigation                                        |
| --------------------------------- | ------------------------------------------------- |
| GGUF size (~2–3 GB)               | Embed only 3B+embed; 32B outside the package      |
| Heterogeneous CUDA / drivers      | `nvidia-smi` prerequisite; no CPU fallback in 0.1 |
| Confusion about an “installed OS” | Explicit Preview banner in the UI                 |


---



## Phase P5 — First-class GPU + polish



### Objective

Fully leverage the GPU/NPU as a first-class citizen of the scheduler, support
multi-GPU, and apply general polish. **Output: Agent OS v1.0.**

### Deliverables


| #    | Deliverable                | Description                                                                                      |
| ---- | -------------------------- | ------------------------------------------------------------------------------------------------ |
| P5.1 | Mature continuous batching | vLLM-like, deep integration with the native scheduler                                            |
| P5.2 | Multi-GPU pipeline         | Distribution of layers across GPUs (pipeline parallelism)                                        |
| P5.3 | Native AccelDevice         | Replace the virtio gateway with a native trait if P4 required it                                 |
| P5.4 | Advanced UI                | Accessibility (F-UI-08); Preview already ships the egui transparency panel + control bar (PC.11) |
| P5.5 | Validated aarch64 port     | Stable execution on at least one target ARM64 machine                                            |
| P5.6 | Stabilization & release    | Fixes, documentation, global acceptance criteria (`docs/functional-specs.md` §9)                 |


> P5 status (15/08/2026): **P5.1 passed** on the development host with
> `aos-gate-p5` — 8 streams 8/8 at ×0.77 wall time vs. single stream
> (NFR-04). Gaps: P5.2 multi-GPU not testable (1× RTX 4080 SUPER), P5.3
> AccelDevice = bare metal (ADR 0001), P5.5 aarch64 deferred. P5.4
> transparency + control bar shipped in Preview egui (PC.11); accessibility
> remains. Dispatcher: gathering window + `generate_batch` (packed prefill,
> unified KV). The single-stream path remains `generate()` (P1).



### Exit gates (Gate P5)

- [x] 8 simultaneous inference streams with < 20% degradation vs. single stream (NFR-04) — `aos-gate-p5`**: 8/8, wall ×0.77 vs. single stream (216 ms → 168 ms)**
- [ ] Multi-GPU: one model distributed over 2 GPUs with a functional pipeline — **gap: 1 physical GPU on the development host**
- [ ] All global acceptance criteria from `docs/functional-specs.md` §9 checked
- [ ] aarch64 port validated on a target machine

---



## Preview increments (P03–P09)

These phases sit **on top of** the PC host stack. They do **not** replace
P0–P5 / PV / PC and they are **not** a P6 gate. Detail lives in
`docs/phases/phase-preview-0n.md` (French mirrors under `docs/fr/phases/`).
Catalogue of what shipped: [FEATURES.md](FEATURES.md). Priorities: E1–E19 in
[evolution-roadmap.md](evolution-roadmap.md).

### P03 — Preview 0.3.0 (E1–E5) — done

Prove the OS thesis in the tester UI and widen the cohort.

| # | Evolution | Deliverable |
|---|-----------|-------------|
| P03.1 | E5 | Live TTFT / tok/s / VRAM in sidebar + Models |
| P03.2 | E4 | Caps tab: `cap.list` + revoke (audited) |
| P03.3 | E1 | CPU-only boot + `cpu` first-run pack + CPU packaging |
| P03.4 | E2 | Cap-gated `schedule.*` + Settings UI |
| P03.5 | E3 | Dual-surface `tasks` module + Tasks tab |

### P04 — Preview 0.4.0 (E6 / E7-lite / E10-lite) — done

| # | Evolution | Deliverable |
|---|-----------|-------------|
| P04.1–P04.2 | E6 | Typed memory graph + Memory UI + structured bootstrap |
| P04.3 | E7-lite | Encrypted `vault.enc`; Settings secrets; MCP `${secret:…}` |
| P04.4 | E10-lite | Cap review on `module.install`; MCP example |
| P04.5 | Docs | Sibling-bridge contract; four CUDA/CPU artefacts |

### P05 — Preview 0.5.0 (E14) — done

Opt-in post-turn `mem.extract` → `mem.user.remember` + E6
`updates`/`supersedes`. Secret filter (never auto-store keys). Settings
toggle (default on). `user.ask` mid-task.

### P06 — Preview 0.6.0 (E8 / E7-keyring / E10) — done

| # | Evolution | Deliverable |
|---|-----------|-------------|
| P06.1–P06.2 | E8 | JSON Schema export `mem.*` / `secrets.*`; HTTP JSON ↔ CBOR contract (no live daemon) |
| P06.3 | E7 | Vault master key in OS keyring (CredMan / Secret Service; file 0600 fallback) |
| P06.4 | E10 | Signed local `share/modules/catalogue.yaml`; hash check on install |
| P06.5 | Stretch | Chat Stop → `model.cancel`; clipboard copy |

### P07 — Preview 0.7.0 (E15) — done

Host-rendered **closed widget tree** (`declarative_ui`) in egui. Installed
modules get a generic tab without a webview and without a hardcoded shell
tab. Vocabulary: `column`, `row`, `heading`, `text`, `markdown`, `stat_row`,
`table`, `line_chart`, `form`, `button`. Notes/Tasks stay hardcoded.

### P08 — Preview 0.8.0 (E16 + E17 + E15 widgets + Providers) — done

**Goal:** local **image generation** and **audio (TTS) generation** as
first-class Model Subsystem workloads, a **unified CPU/GPU host artefact**
(UI switch + load-based auto), **complete module uninstall** from the tester
UI (F-MOD-01), an **E15 widget vocabulary expansion**, a **Providers** tab
(F-MDL-04), **one install command
per OS**, plus a **cleanup / refactor
pass** before tag. Not a cloud-only sidecar. Not video. Not always-on voice
(leave STT / 24/7 voice to the sibling). Not a webview.

Why this belongs in the OS: diffusion and TTS **compete for VRAM** with the
loaded LLM (F-PLC-06 eviction / F-PLC-09 refuse-with-alternative). Agents
must not get ambient “can generate media”; they need `media.generate` +
`fs.write` on the download tree. Device policy is the same Placement Manager,
not a second installer. Module authors still cannot invent widget kinds;
0.8 grows the **host** closed list (typed `form` fields, select/radio,
`bar_chart`, `image`/`audio`). P3 already speaks OpenAI-compat; 0.8 exposes
**named providers** (cloud + loopback) in the UI instead of one Settings key.

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P08.1 | E16 registry | Image + TTS entries in the model catalogue / first-run offerings; Placement Manager treats them as evictable shards vs the LLM | done |
| P08.2 | E16 image | Intent `media.image.generate` (prompt → PNG under `/downloads`); cap review; audit | done |
| P08.3 | E16 audio | Intent `media.audio.generate` (text → WAV/OGG TTS); same cap family; audit | done |
| P08.4 | E16 surface | Chat shows the image / plays the clip (`image` / `audio` kinds in P08.11) | done |
| P08.5 | E17 device | One Win + one Linux artefact; Settings gpu/cpu/auto without reinstall; auto follows VRAM/CPU load (hysteresis) | done |
| P08.6 | E16 packs | Optional media packs (download, not baked into the zip); same download fetches sd.cpp / piper into `bin/` if missing; GPU preferred for image | done |
| P08.7 | F-MOD-01 | Uninstall any non-bundled module from the UI; revoke caps; drop E15 tab; audit | done |
| P08.8 | Hygiene | Cleanup + refactor of Preview host crates (dead code, splits, naming); no behavior change | done |
| P08.9 | Install CLI | One documented command per OS: download + sha256 + overlay into the stable prefix | done |
| P08.10 | Docs / ship | Phase docs, FEATURES/STATUS/TESTER, version 0.8.0, site, packaging | done |
| P08.11 | E15 widgets | Closed vocabulary: typed `form` + `select` / `radio` / `checkbox` / `textarea` / `bar_chart` / `image` / `audio` | done |
| P08.12 | F-MDL-04 | Providers tab: OpenAI-compat cloud (OpenRouter, OpenAI, Anthropic, DeepSeek, z.ai) + local (Ollama, vLLM, LM Studio) | done |

Sequencing: P08.1 → P08.2 ∥ P08.3 ∥ P08.5 ∥ P08.7 ∥ P08.11 ∥ P08.12 → P08.4 → P08.6 → P08.8 → P08.9 → P08.10.  
Detail: [phases/phase-preview-08.md](phases/phase-preview-08.md).

**Out of P08:** video generation, cloud image APIs as the default path,
always-on microphone / STT, messaging channels. Mid-token device migrate
without cancel is **P09 / E18**, not abandoned. Extra image families and
sd.cpp / Piper options are **P09 / E19**. Also out: E7 TPM, live HTTP
sibling daemon, E9 / P5.2 (needs a second GPU), E13 compositor, PC cohort
close, macOS, bare metal, native Anthropic Messages / Gemini / Bedrock
(OpenAI-compat presets only).

### P09 — Preview 0.9.0 (E18 + E19) — done

**Goal:** keep a **live** `model.infer` stream when switching CPU ↔ GPU
(UI pin or `auto` load / E16 VRAM pressure), and **extend local media
generation**. 0.8 already switches by cancel + `aos-modeld` restart, and
hard-codes sd.cpp `-W 512 -H 512 --steps 20` plus Piper defaults on a
single SD 1.5 pack / two voices. 0.9 migrates KV/state so tokens already
shown stay; if migrate fails: 0.8 fallback, audited. **E19** adds a
**closed** option schema (unknown keys refused — no raw argv from agents),
extra optional image packs (Flux2-class, Ideogram4-class) and extra Piper
voices, plus Settings / intent to pick them. Offering-owned extras
(VAE / CLIP / T5 / LoRA) live on the catalogue, not as user-typed paths.
Chat gets **host media plugins**: an Image studio page (advanced sd.cpp
knobs) and an in-chat TTS options card — not WASM marketplace plugins,
not a webview.

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P09.1 | E18 migrate | Active infer CPU ↔ GPU without aborting the stream | done |
| P09.2 | E18 policy | Live UI/`auto` uses migrate; 0.8 cancel+restart is fallback | done |
| P09.3 | E19 schema | Closed option objects on `media.*` + Settings; allowlisted sd.cpp / Piper flags | done |
| P09.4 | E19 catalogue | Extra optional image packs (Flux2, Ideogram4) + extra Piper voices; `extra_files`; VRAM | done |
| P09.5 | E19 surface | Models / Settings pick default pack + options; `/image` and TTS honor them | done |
| P09.8 | E19 chat plugins | Image studio page (style / LoRA / VAE / …); chat control on images opens it; TTS ask opens an in-chat options card | done |
| P09.7 | Hygiene | Finish P08.8 leftovers: split remaining oversized host modules; leftover bilingual chrome; chat role key | done |
| P09.6 | Docs / ship | FEATURES/STATUS/TESTER, version 0.9.0, site, packaging | done |

Sequencing: P09.1 → P09.2 ; P09.3 ∥ P09.4 → P09.5 ∥ P09.8 → P09.7 → P09.6. Depends on P08 / E16+E17.  
Detail: [phases/phase-preview-09.md](phases/phase-preview-09.md).

### Remaining after 0.9 (not a new P6)

P09 is the next Preview increment after 0.8. The rest is scheduled when
hardware or a daemon exists; still Horizon B / C:

| Item | Notes |
|------|--------|
| **E7 TPM** | Hardware envelope of the vault master key (keyring already shipped in 0.6) |
| **E8 live** | HTTP sibling daemon if/when scheduled (contract shipped in 0.6) |
| **E9 / P5.2** | Multi-GPU pipeline when a second GPU exists |
| **P5.3 / E11** | `AccelDevice` + bare metal (after PV.4) |
| **P5.5** | Validated aarch64 host |
| **P5.4 remainder** | Accessibility (F-UI-08) |
| **PC cohort** | 3 Win + 1 Linux testers; independent of Preview increments |
| **E12 / E13** | Horizon C (cognitive preemption, compositor / optional webview) |

---



## Dependency matrix between phases

```
P0 (simulator)
 └──> P1 (real Model Subsystem)      [P0 validates the placement algorithm]
       └──> P2 (Modules + memory)    [P1 provides IPC and the agent runtime]
       │     └──> P3 (Remote + security) [P2 provides the sandbox and audit]
       │           └──> P4 (host userspace caps) [P3 freezes the interfaces]
       │                 ├──> P5 (first-class GPU, host) [parallel]
       │                 ├──> PV (seL4 VM, without GPU) [kernel port]
       │                 │     └──> bare metal (product) [PV green + AccelDevice P5.3]
       │                 └──> PC (installable Preview 0.1)
       │                       └──> P03 → P04 → P05 → P06 → P07 → P08 → P09
       │                             (Preview increments on the host; not P6)
       └──> (P1.7 aarch64, parallel with P1, non-blocking)
```

**Critical points**:

- P1 depends on P0 (algorithm validation before writing the real Placement Manager)
- P4 depends on P3 (do not port interfaces that are still changing)
- **PV and P5 are parallel** (ADR 0001): GPU on the host, kernel in the VM
- Bare metal waits for a green PV gate, not GPU passthrough from Windows
- **P03–P09 do not wait for the PC cohort gate**; they must not invent a P6 number
- P08 (E16/E17) depends on P1 Model Subsystem + P07 E15 if widget kinds are added; it does **not** depend on E9 / TPM / live sibling HTTP
- P09 (E18 + E19) depends on P08 E16 engines + E17 cancel+restart (migrate and extra media packs ship on that base)

---



## Resources and prioritization



### Workstream breakdown


| Workstream                    | Responsibilities                                | Main phases                         |
| ----------------------------- | ----------------------------------------------- | ----------------------------------- |
| **Kernel & Security stream**  | Microkernel, caps, IPC, drivers                 | P0, P4, **PV**, P5                  |
| **Models & Inference stream** | Model Subsystem, placement, scheduler, backends | P0, P1, P3, P5, **P08**, **P09**    |
| **Agents & UX stream**        | Agent Runtime, UI, modules, memory, audit       | P1, P2, P3, P5, **PC**, **P03–P09** |


A team of 3–5 people can cover these 3 streams with rotations; the phases are
designed to be mostly sequential but with partial overlaps (e.g. P1.7 aarch64
in parallel with P1).

### Priorities by phase (reminder)

The `Must` requirements in `docs/functional-specs.md` must be **all covered
by the end of P3** (except those explicitly tied to the microkernel, covered
in P4). `Should` and `Could` requirements are distributed across P4/PV/P5 or
deferred if necessary. Preview increments (P03–P09) cover E* product
priorities on the host without waiting for remaining P5.2 / PV.4 / PC cohort.

---



## Tracking and indicators

- **Gate review**: demonstration under real conditions at the end of each phase, not a slideware presentation
- **Metrics tracked continuously**: embedded TTFT, tok/s under offload, IPC latency, test coverage rate
- **ADR**: every structural decision (UI choice, microkernel, weight format) documented in `adr/` before implementation
- **Spec updates**: every gap discovered during development is fed back into `docs/functional-specs.md` or `docs/technical-specs.md` with a version bump

---



## Related documents

- `docs/functional-specs.md` — product requirements
- `docs/technical-specs.md` — technical architecture
- `docs/FEATURES.md` — shipped Preview catalogue (currently 0.8.0)
- `docs/STATUS.md` — delivered-phase summary
- `docs/vision.md` — framing and open directions
- `docs/competitive-landscape.md` — agentic OS / runtime survey (August 2026)
- `docs/evolution-roadmap.md` — post-landscape priorities E1–E19 (not a P6 gate)
- `docs/phases/phase-preview-03.md` … `phase-preview-09.md` — Preview increment plans
- (published ADRs): `adr/0001-microkernel.md` (P4 host + **phase PV** seL4 VM), `adr/0002-model-placement.md` (P0), `adr/0003-ui-framework.md` (accepted: egui), `adr/0005-offload-etat-de-l-art.md` (pre-P1)

