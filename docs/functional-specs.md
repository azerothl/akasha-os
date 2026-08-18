# Functional specifications — Agent OS

**Language:** English | [Français](fr/specs-fonctionnelles.md)

> Version: 0.3  
> Date: 15/08/2026  
> Status: draft  
> Reference: `docs/vision.md`  
> Changes v0.7.0: Preview 0.7.0 — F-UI-10 / F-EXT-07 host-rendered declarative module UI (E15); not a webview.
> Changes v0.6.0: Preview 0.6.0 — E8 schema export + HTTP↔bus contract; E7 OS keyring; E10 signed local catalogue.
> Changes v0.4.0: Preview 0.4.0 — typed memory graph, secrets vault, module cap review; appendix §12.
> Changes v0.3.1: Preview 0.3.0 — notes package resync, in-app troubleshoot, Split-Flap public site; appendix §12.
> Changes v0.3: Preview 0.1.2 coverage — F-AGT-11, F-NET-01/02, F-UI-09; appendix §12.  
> Changes v0.2: completeness review cross-checked with `docs/technical-specs.md` — clarification of the single-user scope, addition of F-BOOT-06, F-AGT-10, F-FS-05, F-SEC-07, F-SEC-08, clarifications to NFR-04 and glossary.

---

## 1. Product objective

Design an **agent-native operating system** (hereafter **Agent OS**) that:

1. executes **intelligent agents** with a high degree of controlled freedom of action;
2. offers a clear, transparent, and interruptible **user experience**;
3. is **extensible** through modules/applications consumable by **both** agents **and** humans;
4. executes **embedded local AI models** from the first boot, with **remote models** as a complement;
5. **intelligently dispatches** large models between **RAM, GPU/NPU, and disk** to maximize performance on heterogeneous hardware.

Agent OS is **not** merely an agent runtime placed on Linux/Windows: it is a system whose primitives (capabilities, memory, scheduler, IPC, storage) are designed for agentic and inference workloads.

---

## 2. Scope

### 2.1 Included (v1)

- Concurrent multi-agent runtime
- Unified management of local and remote models
- Embedded local models for bootstrap / offline operation
- Dispatch of model weights between RAM / GPU-NPU / disk
- Capability-based security model
- Extensible modules/apps (dual agent + human surface)
- Dual UI (direct + conversational)
- Audit, reversibility, graduated autonomy
- Semantic filesystem (enriched, not exclusive)

### 2.2 Out of scope (v1)

- Complete rewriting of all hardware drivers from scratch
- Native Windows/Linux binary compatibility (apart from a possible later compatibility layer)
- Complete public marketplace (planned for a later phase; local registry in v1)
- Formal support for all cloud backends (priority: OpenAI-compatible APIs + local backends)
- **Simultaneous multi-user** operation (separate accounts with complete OS isolation): v1 is **single-primary-user**; multiple concurrent agents are supported, not multiple isolated human accounts
- Economic model for module distribution (payment, licenses): the v1 registry is local and free

### 2.3 Assumptions

- Target v1 hardware: x86_64 and/or aarch64, with or without GPU/NPU
- At least one persistent storage device (SSD recommended)
- Network optional: the system must boot and operate **offline** with embedded models
- **One single primary human user** per Agent OS instance in v1; this same user carries the “System Administrator” role by default. Strict multi-account separation is a future evolution (see technical roadmap)

---

## 3. Actors

| Actor | Description |
|--------|-------------|
| **Human user** | Person who interacts through the direct or conversational UI, delegates tasks, takes back control, and configures policies |
| **Agent** | Autonomous software entity executed by the runtime, acting through delegated capabilities |
| **Module / Application** | Installable package exposing tools (agents) and/or UI (humans) |
| **AI model** | LLM / VLM / embedding / other local or remote model consumed by the runtime |
| **System Administrator** | Configures global policies, quotas, default models, and trust |
| **Supervisor agent** | System agent responsible for prioritizing notifications, multi-agent mediation, and guardrails |

> Scope note (v1): the “System Administrator” actor is not a separate account — it is the extended role carried by default by the primary human user (see §2.3). The distinction becomes relevant again if multi-user support is introduced in a later version.

---

## 4. Key concepts

### 4.1 Capability

An unfalsifiable, revocable, possibly temporary token authorizing a specific action (e.g.: read file X, call model Y, open a socket to Z). Every action by an agent goes through a capability.

### 4.2 Intent

A high-level structured request issued by an agent or a human, decomposed by the system into a chain of low-level operations (semantic syscalls).

### 4.3 Cognitive context

An agent’s state: working memory, history, active capabilities, and tasks in progress. Savable / restorable (cognitive context switch).

### 4.4 Model backend

Inference provider:
- **Embedded local**: shipped with the OS, available offline
- **Installed local**: downloaded/added by the user
- **Remote**: network API (cloud or private server)

### 4.5 Model Placement

Strategy for distributing a model’s layers/weights between:
- **GPU / NPU VRAM**
- **System RAM**
- **Disk** (offload / layer streaming)

### 4.6 Dual-surface module

Single package exposing:
- a **tool schema** for agents (function calling / semantic IPC)
- a **rendering interface** for humans

---

## 5. Functional requirements

### 5.1 Boot and bootstrap

| ID | Requirement | Priority |
|----|----------|----------|
| F-BOOT-01 | The OS boots and reaches a usable state **without a network connection** | Must |
| F-BOOT-02 | On first boot, at least one **embedded local model** is available for the system assistant and initialization | Must |
| F-BOOT-03 | The user can complete onboarding (language, preferences, trust policies) through a conversational **or** classic UI | Must |
| F-BOOT-04 | If a GPU/NPU is detected, the initial placement of embedded models uses it automatically when possible | Should |
| F-BOOT-05 | If GPU loading fails, automatic CPU/RAM/disk fallback without blocking boot | Must |
| F-BOOT-06 | The system can be updated (system image) with automatic rollback if post-update boot fails | Should |

### 5.2 Agent runtime

| ID | Requirement | Priority |
|----|----------|----------|
| F-AGT-01 | Execute multiple agents in parallel with capability-based isolation | Must |
| F-AGT-02 | Create, suspend, resume, and stop an agent | Must |
| F-AGT-03 | Save and restore an agent’s cognitive context (preemption) | Must |
| F-AGT-04 | Enable multi-agent collaboration through capability-controlled shared memory | Should |
| F-AGT-05 | An agent may perform only actions for which it holds a valid capability | Must |
| F-AGT-06 | The user can interrupt or redirect (steer) a running agent without losing useful context | Must |
| F-AGT-07 | Every significant agent action is recorded in a consultable audit trail | Must |
| F-AGT-08 | Side-effecting actions are **reversible by default** (transaction / snapshot / undo) when technically possible | Must |
| F-AGT-09 | Graduated autonomy: an agent’s capability level can evolve according to a trust score and user policies | Should |
| F-AGT-10 | A system supervisor agent can arbitrate resource or priority conflicts between concurrent agents (beyond simple notification) | Should |
| F-AGT-11 | Before tool use, the runtime classifies the goal (`task.assess` simple/complex) and consults memory first (`mem.bootstrap`) | Should |

### 5.3 AI model management

| ID | Requirement | Priority |
|----|----------|----------|
| F-MDL-01 | Support **local** and **remote** backends through a unified API | Must |
| F-MDL-02 | Ship **embedded local models** by default (at minimum: a small instruct LLM + an embedding model) | Must |
| F-MDL-03 | Allow adding, updating, and deleting local models | Must |
| F-MDL-04 | Allow configuration of remote backends (URL, keys, available models) | Must |
| F-MDL-05 | Intelligent routing: automatically choose local vs. remote according to availability, latency, cost, privacy policy, and task size | Should |
| F-MDL-06 | **Complete offline** operation with only local models | Must |
| F-MDL-07 | The user can force a backend (local only / remote preferred / remote only) per agent or globally | Must |
| F-MDL-08 | Display model status: loaded, partially offloaded, on disk, remote, in error | Must |
| F-MDL-09 | Support simultaneous multi-model operation (e.g.: LLM + embeddings + VLM) | Must |
| F-MDL-10 | Quota and prioritization of model access between agents (e.g. token or inference-time budget per period, per agent or per module) | Should |

### 5.4 Dispatch of large models (RAM / GPU / Disk)

| ID | Requirement | Priority |
|----|----------|----------|
| F-PLC-01 | The system can **distribute a model’s weights** between VRAM (GPU/NPU), system RAM, and disk | Must |
| F-PLC-02 | **Automatic** placement according to available resources, agent priority, and user policies | Must |
| F-PLC-03 | Configurable **manual** placement (e.g.: “layers 0-20 GPU, rest RAM”, or predefined profiles) | Should |
| F-PLC-04 | **Streaming / offloading** of layers from disk to RAM/GPU on demand during inference | Must |
| F-PLC-05 | Intelligent prefetching of layers likely to be used | Should |
| F-PLC-06 | Dynamic eviction: if a priority agent needs VRAM, move/unload layers from a lower-priority model | Must |
| F-PLC-07 | Support multi-GPU and multi-device operation (inter-GPU distribution) when hardware permits | Could |
| F-PLC-08 | Exposed metrics: VRAM/RAM/disk usage per model, layer cache hit rate, tokens/s, TTFT latency | Must |
| F-PLC-09 | Never block the entire system when an overly large model is requested: controlled degradation or explicit refusal with an alternative | Must |
| F-PLC-10 | Predefined placement profiles: `latency` (max GPU), `balanced`, `memory-saver` (max disk offload), `cpu-only` | Should |

### 5.5 Agentic memory

| ID | Requirement | Priority |
|----|----------|----------|
| F-MEM-01 | Working memory per agent (short-term context) | Must |
| F-MEM-02 | Long-term / episodic memory with semantic search (embeddings) | Must |
| F-MEM-03 | Inter-agent shared memory under capability control | Should |
| F-MEM-04 | Semantic eviction policy (relevance) in addition to conventional policies (LRU) | Should |
| F-MEM-05 | The user can inspect, edit, and delete an agent’s memory | Must |

### 5.6 Storage and filesystem

| ID | Requirement | Priority |
|----|----------|----------|
| F-FS-01 | Conventional hierarchical storage (paths) for performance and clarity | Must |
| F-FS-02 | Semantic addressing: retrieve data by description / intent | Should |
| F-FS-03 | Snapshots / versions to enable undo of agent actions | Must |
| F-FS-04 | Isolation of data spaces by capabilities (no ambient access) | Must |
| F-FS-05 | Each piece of data can carry a **sensitivity classification** (e.g. public / private / secret), assigned by default (folder inheritance) and modifiable by the user, and used by routing and sharing policies | Must |

### 5.7 Modules and applications

| ID | Requirement | Priority |
|----|----------|----------|
| F-MOD-01 | Install / uninstall / update a module | Must |
| F-MOD-02 | Each module declares a **dual manifest**: agent tools + human UI | Must |
| F-MOD-03 | Dynamic discovery of a module’s capabilities (schema introspection) | Must |
| F-MOD-04 | Sandboxed execution of modules (no system access outside declared capabilities) | Must |
| F-MOD-05 | Agents and humans consume the **same** module through adapted surfaces | Must |
| F-MOD-06 | Local module registry in v1; network distribution in a later phase | Must |

### 5.7bis Agent extensions (skills / modules)

| ID | Requirement | Priority |
|----|----------|----------|
| F-EXT-01 | An agent can create a declarative **skill** (markdown + tools) under trust governance | Must |
| F-EXT-02 | An agent can request a missing capability via `cap.request` (hot-grant) | Must |
| F-EXT-03 | An agent can scaffold / package a script module (ext-rt) without a Rust toolchain | Must |
| F-EXT-04 | An agent can compile a Rust→WASM module if the toolchain is present (critical capability) | Should |
| F-EXT-05 | `module.install` requires a critical capability + capability review (no more anonymous installation) | Must |
| F-EXT-06 | The tool catalog (`module.describe` + skills) is injected into the agent prompt | Must |
| F-EXT-07 | An agent can author a **declarative module UI** (closed widget tree) consumed by the host; unknown widget kinds are refused | Should |

### 5.8 User interface

| ID | Requirement | Priority |
|----|----------|----------|
| F-UI-01 | **Direct** mode: navigation, settings, file management, agent/model management | Must |
| F-UI-02 | **Conversational** mode: dialogue with the system assistant and agents | Must |
| F-UI-03 | Both modes coexist and operate on the same data/capabilities | Must |
| F-UI-04 | Reasoning transparency: display why an agent acted (levels of detail) | Must |
| F-UI-05 | Human control can be resumed at any time (pause / cancel / steer) | Must |
| F-UI-06 | Resource dashboard: CPU, RAM, VRAM, disk, loaded models, active agents | Must |
| F-UI-07 | Prioritized notifications (supervisor agent), no spam | Should |
| F-UI-08 | Basic accessibility (contrast, text size, keyboard navigation) | Should |
| F-UI-09 | Persisted user preferences (language, routing, trust, network, agent defaults, search engine) editable from a Settings surface | Must |
| F-UI-10 | Installed modules with `ui.mode=declarative_ui` get a host-rendered human surface (closed widget vocabulary: form, table, stats, line chart); no HTML/JS webview in Preview | Should |

### 5.9 Security, privacy, trust

| ID | Requirement | Priority |
|----|----------|----------|
| F-SEC-01 | Every action passes through the capability model | Must |
| F-SEC-02 | Immediate revocation of a capability or an agent | Must |
| F-SEC-03 | Privacy policy: prohibit sending certain data to remote backends | Must |
| F-SEC-04 | Secrets (API keys) stored encrypted, never exposed in plaintext to unauthorized agents | Must |
| F-SEC-05 | Audit trail cannot be altered by application agents | Must |
| F-SEC-06 | Isolation between unrelated agents (no context leakage) | Must |
| F-SEC-07 | The system can require **explicit human confirmation** before executing an action classified as sensitive by policy (distinct from simple after-the-fact auditing) | Must |
| F-SEC-08 | An agent’s or module’s outbound network access (egress) is controlled by explicit capability (authorized host/domain); denied by default outside configured model backends | Must |

### 5.9bis Network tools (Preview)

| ID | Requirement | Priority |
|----|----------|----------|
| F-NET-01 | Opt-in web search with selectable engine (`auto` / Brave / DuckDuckGo / Bing), refused under strict offline | Must |
| F-NET-02 | Read a page as HTML→text without executing JavaScript (`web.browse`), under the same egress policy | Should |

### 5.10 Observability and administration

| ID | Requirement | Priority |
|----|----------|----------|
| F-OBS-01 | Structured system logs | Must |
| F-OBS-02 | Inference and model placement metrics | Must |
| F-OBS-03 | Administrator view: global policies, quotas, default models | Must |
| F-OBS-04 | Audit export for external analysis | Should |

---

## 6. Main user journeys

### 6.1 First boot (offline)

1. OS boot  
2. Load the embedded model (automatic placement according to hardware)  
3. System assistant guides onboarding  
4. Create the user profile and basic policies (privacy, agent autonomy)  
5. Desktop / shell ready, system agents active  

### 6.2 Delegate a task

1. The user describes an intent (conversational UI or form)  
2. The system creates/assigns an agent with the minimum required capabilities  
3. The agent plans and executes; the user sees progress and reasoning  
4. Sensitive actions require confirmation according to policy  
5. Result delivered; undo possible if applicable  

### 6.3 Add a large local model

1. The user imports or downloads a model (e.g. quantized 70B)  
2. The **Model Placement Manager** analyzes size, VRAM, RAM, and disk  
3. Proposes a placement profile (`balanced` by default)  
4. Progressive loading; visible metrics  
5. The model becomes selectable by agents  

### 6.4 Configure a remote backend

1. The user adds endpoint + credentials  
2. Connectivity test and model discovery  
3. Policy: which agents / which data may use it  
4. Automatic or manual routing according to preferences  

### 6.5 Install a module

1. Select a module in the local registry  
2. Review requested capabilities (equivalent to a permissions store)  
3. Sandboxed installation  
4. Tools visible to agents; UI visible to humans  

---

## 7. Important business rules

1. **Offline-first**: no critical shell/assistant function may depend on the network.  
2. **Least privilege**: minimum capabilities by default; explicit or progressive-trust expansion.  
3. **Privacy by default**: local data is not sent to a remote backend without an authorizing policy.  
4. **Graceful degradation**: insufficient VRAM → RAM/disk offload; no network → local models; model unavailable → alternative or clear message.  
5. **Reversibility**: prefer transactional operations for any agent side effect.  
6. **One unified model API**: agents and UI do not see the difference between local/remote except for deliberately exposed status information.

---

## 8. Non-functional requirements (product view)

| ID | Category | Requirement |
|----|-----------|----------|
| NFR-01 | Perf | TTFT (time to first token) of the embedded model < 2s on the reference machine (to be defined in technical specifications) after warm-up |
| NFR-02 | Perf | The UI shell remains responsive (> 30 FPS / interactions < 100ms) even under inference load |
| NFR-03 | Reliability | A crash of an agent or model backend does not crash the kernel or system UI |
| NFR-04 | Scalability | Support ≥ 32 concurrent lightweight agents, including up to 8 simultaneous inference streams, on the reference machine (see docs/technical-specs.md §13) |
| NFR-05 | Security | No privilege escalation through a sandboxed module |
| NFR-06 | Privacy | “Local only” mode guaranteed auditable |
| NFR-07 | UX | Onboarding completable in < 10 minutes offline |
| NFR-08 | Extensibility | Add a module without recompiling the OS |
| NFR-09 | Observability | Placement + inference metrics available in real time |
| NFR-10 | Portability | Builds targeting x86_64 and aarch64 (roadmap) |

---

## 9. Global acceptance criteria (v1)

- [ ] Offline boot with a functional conversational assistant (embedded model)
- [ ] Multi-agent execution isolated by capabilities
- [ ] Add a local model larger than VRAM and successfully run inference through RAM/disk offload
- [ ] Configure a remote backend and switch between local/remote
- [ ] Install a dual-surface module consumed by an agent and a human
- [ ] Interrupt / undo of an agent action visible in the UI
- [ ] Operational resource dashboard and audit trail
- [ ] “Local only” policy preventing all model network calls
- [ ] An action classified as sensitive triggers a blocking human confirmation request before execution
- [ ] A module without explicit network capability cannot reach any remote host

---

## 10. Glossary

| Term | Definition |
|-------|------------|
| Agent OS | Operating system that is the subject of this specification |
| Capability | Delegated unitary action right |
| Intent | High-level semantic request |
| Model Placement | Distribution of a model’s weights across GPU/RAM/disk |
| Offload | Moving model layers out of VRAM (to RAM or disk) |
| TTFT | Time To First Token |
| Dual-surface | Module exposing an agent API + human UI |
| Cognitive context switch | Saving/restoring an agent’s complete state |
| Sensitivity class | Label (public / private / secret) carried by data, used by routing and sharing policies |
| Required confirmation | Blocking mechanism requesting explicit human validation before executing a sensitive action |
| Network egress | Outbound network traffic initiated by an agent/module; controlled by a dedicated capability |
| Trust score | Indicator of an agent’s behavioral history used to graduate its autonomy (details: docs/technical-specs.md §4.7) |

---

## 11. Related documents

- `docs/vision.md` — founding reflection  
- `docs/technical-specs.md` — technical specifications  
- `docs/FEATURES.md` — shipped Preview 0.7.0 catalogue  
- (future) `adr/` — Architecture Decision Records  

---

## 12. Preview 0.7.0 coverage (host)

Mapping of this spec onto the **installable host Preview** (not the bootable OS).
Detail: `docs/FEATURES.md`.

| Spec IDs | Preview status |
|----------|----------------|
| F-BOOT-01–05 | Hardware probe + model setup + offline chat; no CPU fallback |
| F-BOOT-06 | Non-destructive Release overlay (apply on next launch) |
| F-AGT-01–08, 11 | Goal loop, caps, audit, steer, `task.assess`, memory-first |
| F-AGT-09–10 | Trust low/medium + confirm; supervisor restart of auditd |
| F-MDL-01–09 | Local llama.cpp + optional remote; routing local_only / balanced |
| F-MEM-01–02, 05 | Working / episodic / user facts; Memory tab |
| F-MOD / F-EXT | notes.aospkg, ext-rt, declarative skills, `cap.request`; host-rendered `declarative_ui` tabs |
| F-UI-10 / E15 | **Shipped Preview 0.7.0** — host-rendered `declarative_ui` (not webview) |
| F-UI-01–05, 09 | Dual surface + transparency panel + Settings |
| F-SEC-01–08 | Caps, confirm, egress deny-by-default, auditd isolation; vault master key in OS keyring (file 0600 fallback); agents denied `secrets.get` |
| F-MOD catalogue | Signed local `share/modules/catalogue.yaml`; hash check on `module.install` |
| E8 bridge | JSON Schema `docs/bridge/` + HTTP JSON ↔ CBOR contract (no live daemon) |
| F-NET-01–02 | Multi-engine search + `web.browse` |  
