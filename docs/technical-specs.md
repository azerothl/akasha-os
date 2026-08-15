# Technical specifications — Agent OS

**Language:** English | [Français](fr/specs-techniques.md)

> Version: 0.3  
> Date: 15/08/2026  
> Status: draft  
> Reference: `docs/functional-specs.md`, `docs/vision.md`, `docs/FEATURES.md`  
> Changes v0.3: Preview 0.1.2 — `web.browse`, multi-engine `web.search`, egui transparency panel, persisted Settings (`preferences.json`).  
> Changes v0.2: completeness review — addition Supervisor Agent/System Assistant/Trust Manager (§4.5-4.7), sensitivity classification (§6.4), Registry Module (§7), network egress (§9.5), blocking confirmation (§9.4), system updates (§10.1), API Module/API Admin (§11.4-11.5), user profile (§12), concurrent agent targets (§13), accessibility (§8.3), technical glossary and traceability matrix (§21-22).

---

## 1. Vue d'ensemble architecture

### 1.1 Principes directeurs

1. **Microkernel capability-based** for the minimum trust kernel
2. **User space system services** (drivers, filesystem, model runtime, agent runtime)
3. **Semantic IPC** as native bus (typed intentions, not just bytes)
4. **GPU/NPU first-class** in the scheduler and memory manager
5. **Offline-first**: embedded models + mandatory local placement at boot
6. **WASM/WASI** for sandboxed modules
7. **Rust** preferred for TCB (Trusted Computing Base) and critical services

### 1.2 Couches

```
┌─────────────────────────────────────────────────────────────┐
│  Surfaces utilisateur                                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │ UI Directe   │  │ UI Convers.  │  │ CLI / Debug      │  │
│  └──────┬───────┘  └──────┬───────┘  └────────┬─────────┘  │
├─────────┴─────────────────┴───────────────────┴─────────────┤
│  Agent Runtime          │  Module Runtime (WASM)            │
│  - lifecycle agents     │  - sandbox + caps                 │
│  - context switch cog.  │  - double-surface manifest        │
├─────────────────────────┴───────────────────────────────────┤
│  Semantic IPC Bus (intents, schemas, capability tokens)     │
├──────────────┬──────────────────┬───────────────────────────┤
│ Model        │ Memory           │ Storage                   │
│ Subsystem    │ Subsystem        │ Subsystem                 │
│ - registry   │ - working        │ - hierarchical FS         │
│ - backends   │ - episodic/vec   │ - semantic index          │
│ - placement  │ - shared         │ - snapshots / undo        │
│ - scheduler  │                  │                           │
├──────────────┴──────────────────┴───────────────────────────┤
│  System Services (user-space)                                │
│  net │ display │ input │ audit │ policy │ device-mgr         │
├─────────────────────────────────────────────────────────────┤
│  Microkernel                                                 │
│  caps │ IPC │ threads │ vm │ irq │ sched primitives          │
├─────────────────────────────────────────────────────────────┤
│  Hardware : CPU │ RAM │ GPU/NPU │ NVMe/SSD │ net             │
└─────────────────────────────────────────────────────────────┘
```

### 1.2bis Additional services (not detailed in the diagram above)

The layered schema emphasizes the main data path (inference). The following services are housed in the “Agent Runtime” / “System Services” layers but deserve to be mentioned explicitly for functional completeness:

| Services | Housed in | Detailed in |
|---------|-----------|-------------|
| **Module Registry** (module catalog/installation) | System Services, next to the Runtime Module | §7 |
| **Supervising agent** (multi-agent arbitration, notifications) | Runtime Agent, privileged system agent | §4.6 |
| **System Assistant Agent** (default onboarding/shell assistant) | Runtime Agent, privileged system agent | §4.5 |
| **Trust Manager** (trust score, graduated autonomy) | System Services | §4.7 |
| **Network Egress Control** (outgoing network capabilities) | System Services (`net`) + Policy | §9.5 |
| **Admin Service** (global policies, quotas, default templates) | System Services (`policy`) | §11.5 |

### 1.3 Phased implementation strategy

| Phase | Socle | Livrable |
|-------|-------|----------|
| **P0** | Linux host (dev) or microVM | Agentic Services + Complete Model Subsystem (userspace) |
| **P1** | Existing microkernel (seL4 / Redox / Zircon-like) | Port of services, native caps |
| **P2** | Dedicated Agent OS kernel (Rust) | Deep GPU sched integration + native semantic FS |

> The technical spec below describes the **target P1/P2**. P0 validates the algorithms (placement, batching, logical headings) without waiting for the final kernel.

---

## 2. Microkernel

### 2.1 Responsibilities (minimum TCB)

- Management of **capabilities** (creation, derivation, revocation, rights bits)
- Synchronous/asynchronous **IPC** between tasks
- Low level threads / scheduling (priorities, time slices)
- Virtual memory (address spaces, grant/map via caps)
- Delegation of interrupts to user-space drivers
- Horloge, timers

### 2.2 What is NOT in the kernel

- File system
- Network stack
- GPU drivers (excluding memory access primitive/IRQ)
- Runtime models/agents
- UI

### 2.3 Capabilities model

```text
Cap = {
  object_id,          // référent
  rights,             // bitmask : READ|WRITE|EXECUTE|GRANT|REVOKE|...
  badge,              // contexte opaque pour le détenteur
  ttl_optional,       // expiration
  attenuation_rules   // droits maximaux dérivables
}
```

Primitive operations:
- `mint` / `derive` (rights mitigation)
- `grant` (transfer to another address space)
- `revoke` / `revoke_tree`
- `invoke` (call on the referenced object)

Any resource (file, socket, model, shared memory, device) is a **object referenced by cap**.

### 2.4 Semantic IPC

Above raw kernel messages, the **Semantic IPC Bus** imposes:

- **typed** messages (versioned schema, e.g. JSON Schema / CBOR Schema / protobuf-like)
- attachment of **caps** in messages
- request/response + streams correlation
- service discovery (`lookup("model.infer")`)

Example of intent:

```json
{
  "intent": "model.infer",
  "version": 1,
  "payload": {
    "model_ref": "local:embedded-instruct-v1",
    "messages": [{"role": "user", "content": "Résume note.md"}],
    "params": {"max_tokens": 512, "temperature": 0.2}
  },
  "caps": ["cap://fs/read/notes/note.md", "cap://model/use/embedded-instruct-v1"]
}
```

---

## 3. Model Subsystem (critical core)

The Model Subsystem is a privileged **user-space system service**, divided into components.

### 3.1 Components

```
Model Subsystem
├── Model Registry          # catalogue local + distant, métadonnées
├── Backend Manager         # local runtime(s) + remote clients
├── Placement Manager       # dispatch couches RAM/GPU/Disque
├── Inference Scheduler     # files, batching, priorités agents
├── Weight Store            # stockage weights, mmap, cache couches
├── Tokenizer Service       # tokenizers partagés
└── Metrics Exporter        # TTFT, tok/s, VRAM, hit rates
```

### 3.2 Model Registry

Metadata of a model:

```yaml
id: local:llama-q6-32b
name: Llama 32B Q6
modality: text
format: gguf          # gguf | safetensors | onnx | remote-api
source:
  type: local_file
  path: /models/llama-32b-q6.gguf
  sha256: ...
architecture:
  n_layers: 80
  n_params: 32e9
  context_length: 131072
resource_hints:
  weights_bytes: 24000000000
  min_vram_full: 22000000000
  min_ram_full: 26000000000
  supports_layer_offload: true
  supports_quantizations: [q4_k, q5_k, q6_k, q8_0]
capabilities:
  - chat
  - tools
backends_compatible: [llamacpp, candle, triton]
privacy_class: local
```

For a remote model:

```yaml
id: remote:openai:gpt-4.1
source:
  type: remote_api
  endpoint: https://api.openai.com/v1
  protocol: openai_compatible
auth_cap: cap://secrets/openai_key
privacy_class: remote
```

### 3.3 Backends

| Backend | Role | Notes |
|---------|------|-------|
| **Embedded Runtime** | Native local inference (target: llama.cpp-like / candle / ggml) | Mandatory at boot |
| **Advanced Local** | Optional high performance backend (vLLM/TGI type if worn) | Later phase |
| **Remote OpenAI-compatible** | HTTP/S SSE or custom streaming | Cloud or private server |
| **Remote gRPC** | Option for internal clusters | Could |

**Internal unified API** (all backends):

```text
infer(request) -> stream<TokenEvent>
embed(request) -> Vector
list_models() -> []
health() -> Status
cancel(inference_id)
```

### 3.4 Embedded models (bootstrap)

Delivered in the system image under `/system/models/` (logical path):

| Role | Target size | Usage |
|------|--------------|-------|
| `embedded-instruct` | 1B–3B quantified (Q4/Q5) | Shell wizard, onboarding, lightweight agents |
| `embedded-embed` | ~100–500MB | Semantic memory, semantic FS |
| (optional) `embedded-router` | very small | Local vs remote intent/routing classification |

Contraintes :
- loading **guaranteed** in RAM (+ GPU if available) at first boot
- no network dependency
- signature/hash verified at startup

### 3.5 Placement Manager — dispatch RAM / GPU / Disque

#### 3.5.1 Objectif

Run models whose size **exceeds VRAM**, or even RAM, by distributing the **layers** and associated buffers over three thirds:

| Tier | Medium | Access latency | Typical usage |
|------|--------|-----------------|---------------|
| T0 | GPU/NPU VRAM | very weak | hot active layers, KV priority cache |
| T1 | System RAM | low | warm layers, KV overflow |
| T2 | Disque (SSD NVMe) | moyenne | couches cold, weights mmap |

#### 3.5.2 Placement units

- **Layer shard**: a transform layer (or group of layers)
- **KV cache blocks**: paginated (configurable block size, e.g. 16–64 tokens)
- **Embedding / output tables**: placed according to hotness

Each unit has a descriptor:

```text
Shard {
  model_id,
  shard_id,
  kind: Layer | KVBlock | Embed | Other,
  size_bytes,
  residency: VRAM | RAM | DISK,
  pin_count,
  last_use_ts,
  priority_boost
}
```

#### 3.5.3 Algorithme de placement initial

Entries:
- `W` = taille totale weights
- `V_free`, `R_free`, `D_free` = free VRAM/RAM/Disk budgets (after OS reserves)
- profil : `latency` | `balanced` | `memory-saver` | `cpu-only`
- agent constraints (deadline, priority)

Pseudo-code :

```text
function place_model(model, profile, budgets):
  reserve_os_budgets()  # ne jamais tout manger

  if profile == cpu-only or no_gpu:
    assign all layers -> RAM, overflow -> DISK (mmap)
    kv -> RAM
    return plan

  # Score chaque layer par "hotness attendu"
  # (entrée/sortie souvent plus hot; milieu selon modèle)
  layers = score_layers(model)

  sort layers by score desc

  for layer in layers:
    if fits(layer, VRAM) and profile in [latency, balanced]:
      assign layer -> VRAM
    else if fits(layer, RAM):
      assign layer -> RAM
    else:
      assign layer -> DISK

  # KV cache: préférer VRAM puis RAM; jamais DISK sauf mode extrême
  place_kv_policy(profile)

  validate_plan()  # estimer tok/s min; refuser si sous seuil critique
  return plan
```

#### 3.5.4 Execution with offload (runtime)

During the forward pass:

1. For each layer `L` in the order of the graph:
   - si `L` en VRAM → compute direct
- if `L` in RAM → async upload to VRAM (or compute CPU if backend allows it)
- if `L` in DISK → page-in to RAM (mmap + fault) then upload if GPU
2. **Prefetch**: anticipate layers `L+1..L+k` on dedicated DMA stream
3. **Evict**: least recently used layers / low score if memory pressure
4. Double-buffering : calculer `L` pendant transfert de `L+1`

#### 3.5.5 Memory pressure and preemption

Triggers :
- new high priority VRAM request agent
- UI system reports frame drop / low memory
- seuil `vram_watermark_high`

Actions (in order):
1. Reduce batch size / low-priority inference competitors
2. Migrer layers low-priority VRAM → RAM  
3. Migrer layers RAM → DISK  
4. Suspend low-priority inferences (partial state backup if possible)
5. Refuse new uncritical inferences with explicit error + alternatives

#### 3.5.6 Profils de placement

| Profil | VRAM | RAM | Disque | Objectif |
|--------|------|-----|--------|----------|
| `latency` | max layers + KV | overflow | minimal | max tok/s |
| `balanced` | layers hot + KV | layers warm | cold | default |
| `memory-saver` | Minimal KV / micro-batch | little | majority weights | multi-model cohabitation |
| `cpu-only` | 0 | max | overflow | no GPU |

#### 3.5.7 Multi-GPU (later phase)

- pipeline partition (layers 0..k on GPU0, k+1..n on GPU1)
- or tensor parallel if backend supports it
- Placement Manager exposes a unified multi-device plan

#### 3.5.8 Weight Store

- Weights files in **read only**, addressable by offset
- **mmap** for DISK→RAM zero-copy when possible
- Page cache with **LFU + semantic pin** policy (pinned layers for system model)
- Integrity: hash by shard, lazy or load verification
- Optional runtime quantization (downgrade Q6→Q4 under pressure) — Could

### 3.6 Inference Scheduler

Features :
- files by **priority** (system critical > interactive UI > agent high > agent normal > batch)
- **Continuous batching** of compatible requests (same model, same dims) — vLLM continuous batching inspiration
- cooperative preemption at token/layer boundaries
- fair share between agents of equal priority (configurable weights)
- immediate cancellation propagated to the backend

Structure :

```text
InferenceScheduler {
  queues: Map<Priority, Queue<InferJob>>
  running: Set<InferJob>
  model_locks / resource_tokens
  batch_window_us: 50..2000
}
```

#### 3.6.1 Access quotas (F-MDL-10)

Each agent (and each module that invokes a model on its behalf) has a renewable budget of type **token bucket**:

```text
AgentQuota {
  agent_id,
  tokens_per_minute: u32,
  concurrent_inferences_max: u8,
  gpu_time_ms_per_minute: u32,
  burst_allowance: f32       // marge temporaire au-delà du quota nominal
}
```

- Exceeding quota → job demoted to queue `batch` (no abrupt refusal), except priority `system critical`.
- Default quotas set by the Admin Service (§11.5); adjustable by agent via policy.
- Consumption exposed in the metrics (§13) for user transparency.

### 3.7 Local vs. remote routing

```text
function select_backend(req, policy):
  if policy.mode == local_only: return best_local(req)
  if policy.mode == remote_only: return best_remote(req)

  candidates = []
  if local_available(req.model_or_task): candidates += score_local()
  if remote_allowed(req) and network_ok(): candidates += score_remote()

  score = f(latency_est, cost, privacy_risk, load, quality_hint)
  return argmax(candidates) or fail_with_alternatives()
```

> `quality_hint`: task complexity signal provided by the calling agent (e.g. `simple` / `reasoning` / `long_context`) or derived by `embedded-router` (§3.4); influences the preference for a more capable model even if more expensive/latent.

Privacy rules:
- data classification (`public`, `private`, `secret`)
- `secret` → jamais remote
- `private` → remote seulement si policy explicite

---

## 4. Runtime Agent

### 4.1 Execution model

- Each agent = **task group**: logical threads + logical address space + cap set + cognitive state
- **asynchronous** execution (green threads / async runtime) to not block on I/O model
- Isolation: no implicit shared memory between agents

### 4.2 Cognitive State

```text
CognitiveState {
  agent_id,
  working_memory,      // fenêtre contexte courante
  plan_stack,          // sous-buts
  tool_session,        // appels outils en cours
  cap_set_snapshot,
  inference_handles,
  episodic_cursors,    // pointeurs mémoire long terme
  version
}
```

Operations: `suspend`, `resume`, `snapshot`, `restore`, `migrate` (future).

### 4.3 Lifecycle API

```text
agent.create(spec, initial_caps) -> agent_id
agent.start(agent_id)
agent.steer(agent_id, directive)
agent.pause(agent_id)
agent.resume(agent_id)
agent.kill(agent_id)
agent.grant(agent_id, cap)
agent.revoke(agent_id, cap)
```

### 4.4 Tools and modules

An agent invokes a tool via Semantic IPC:

```text
invoke_tool(tool_id, args, caps) -> result
```

The Runtime Module checks:
1. the args diagram
2. that the agent holds the caps required by the manifest
3. quotas

### 4.5 System Assistant Agent

Default system agent, only agent started automatically at each boot (§10):

- uses `embedded-instruct` by default (switches to a more capable model if available and allowed)
- supports onboarding, conversational shell, contextual help
- **does not have implicit high capacities**: despite its “system” status, it obeys the same capacity model as any agent (§2.3); its only special features are (a) guaranteed boot, (b) read access to non-sensitive system metrics to answer user questions
- all his actions are audited like any agent (§9.3)

### 4.6 Supervisory agent (arbitration & notifications)

Privileged System Agent responding to F-UI-07 and F-AGT-10:

```text
SupervisorAgent {
  notification_queue: PriorityQueue<Notification>,
  conflict_log: []
}
```

Responsibilities:
- **Aggregation and prioritization of notifications** (deduplication, grouping by context, silence of non-critical notifications according to user policy)
- **Arbitration of conflicts** between competing agents: incompatible resource requests (e.g. two agents requesting the same file exclusivity), contradictory priorities → decision according to declarative rules (declared priority, seniority, confidence score) then escalated to humans if not resolved automatically
- **Introspective access only** to file metadata (priority, agent_id, requested resource) — **no access to the content/cognitive context** of the arbitrated agents (compliance with F-SEC-06)
- Exposes its decisions via the audit trail and the UI transparency panel (§8.1)

### 4.7 Trust Manager — graduated autonomy (F-AGT-09)

Service which calculates and evolves a **confidence score** per agent (or per agent class/module):

```text
TrustProfile {
  agent_id | agent_class,
  score: f32,                 // 0.0 – 1.0
  history_window,             // fenêtre glissante (ex. 30 jours / N actions)
  factors: {
    success_rate,             // tâches complétées sans erreur
    override_rate,            // fréquence d'annulation/correction humaine
    confirmation_denials,     // demandes de confirmation refusées par l'utilisateur
    age_days
  },
  last_updated
}
```

Fonctionnement :
- The score is recalculated after each significant action (audit event) according to a weighted function of the factors above.
- The score **never automatically** grants a sensitive capacity alone: ​​it defines **tiers** (e.g. `low` / `medium` / `high`) which determine if a capacity request can be:
- granted automatically (sufficient level + non-critical capacity),
- subject to human confirmation (§9.4),
- automatically refused (insufficient level for a critical capacity, whatever the score).
- The user can consult, freeze or reset an agent's score at any time (transparency, no “black box”).
- The Trust Manager is a **consultative** service: the Policy Engine (§9.4) remains the final decision-making authority.

---

## 5. Memory Subsystem

### 5.1 Niveaux

| Level | Implementation | Duration |
|--------|----------------|-------|
| Working | buffer structured in RAM by agent | session/task |
| Episodic | vector store + metadata | long term |
| Shared | shared segments + caps | collaboration scope |
| System | politiques, audit indexes | permanent |

### 5.2 Store vectoriel natif

- embeddings via `embedded-embed` by default
- ANN index (HNSW or equivalent)
- eviction: score = f(semantic relevance, recency, user pin)
- optional encryption at rest for sensitive memories

### 5.3 API

```text
mem.working_get/set
mem.episodic_write(event)
mem.episodic_query(query, k, filters)
mem.shared_open(cap) / read / write
mem.export / wipe (user-facing)
```

---

## 6. Storage Subsystem

### 6.1 Hierarchical FS

- Modern FS logged with **snapshots** (ZFS/btrfs/APFS inspiration)
- ACLs replaced by **caps files**
- Copy-on-write for undo agent

### 6.2 Semantic index

- asynchronous indexing service (text, metadata, embeddings)
- query: classic path **or** intent (`find "contrat bail 2024"`)
- the path remains the source of truth; the index is derived

### 6.3 Undo / transactions

```text
tx = fs.begin_transaction(agent_id)
... opérations ...
fs.commit(tx) | fs.rollback(tx)
```

Named snapshots before high-risk actions (policy).

### 6.4 Sensitivity classification (F-FS-05)

Each FS object carries an extended attribute (xattr-like):

```text
DataClass = public | private | secret
```

Rules :
- **Inheritance** by default from the parent folder (`/home/<user>/secrets/**` → `secret` by default, `/home/<user>/documents/**` → `private` by default).
- **Manual override** possible by the user (file properties, direct UI).
- **Assisted classification** (Should, non-blocking): a lightweight classifier (heuristics + `embedded-embed`) can suggest a class when creating a file (e.g. IBAN type pattern detection, API key); the user validates or corrects.
- Consumed directly by the Policy Engine (§9.4) and the routing model (§3.7, `privacy_risk`): a `intent` which references a `secret` object cannot be served by a `remote` backend, regardless of the policy otherwise.
- Modifiable only via a `fs.reclassify` capability distinct from `fs.write` (prevents an agent from discreetly lowering the sensitivity of data to circumvent a policy).

---

## 7. Module system (WASM)

### 7.1 Components

The module system is divided into two distinct services, like the Model Subsystem (§3.1):

```
Module System
├── Module Registry   # catalogue local : liste, versions, état d'installation, dépendances
└── Module Runtime    # exécution sandboxée WASM, résolution de caps au runtime
```

- **Module Registry**: source of truth on what is installed/available (F-MOD-01, F-MOD-06). Local registry in v1 (file signed `/system/modules/registry.yaml` + `/var/modules/`), synchronization with a remote registry outside v1 scope.
- **Module Runtime**: loads the WASM binary, applies the sandbox (§7.4), routes tool invocations to the module (F-MOD-04, F-MOD-05).

### 7.2 Format package

```text
module.aospkg
├── manifest.yaml
├── module.wasm
├── ui/                 # déclaratif (ex. JSON UI / web bundle sandboxé)
├── schemas/            # tools input/output
├── assets/
└── signatures/
```

### 7.3 Manifeste double-surface

```yaml
name: notes
version: 1.2.0
hash: sha256:...
permissions:
  required_caps:
    - fs.read:/documents/notes/**
    - fs.write:/documents/notes/**
tools:
  - name: notes.create
    description: Créer une note
    input_schema: schemas/create.json
    output_schema: schemas/create_out.json
  - name: notes.search
    ...
ui:
  entry: ui/index.html
  mode: sandboxed_webview  # ou declarative_ui
min_os_api: 1
```

> Example of network capacity for a module requiring external access (e.g. web search): `required_caps: [net.connect:api.example.com:443]` — subject to user review during installation and control of egress (§9.5).

### 7.4 Sandbox

- WASM runtime (adapted WASI preview)
- **no** host access outside injected caps
- CPU/mem/time limits per invocation
- signatures verified before install/load

### 7.5 Declarative skills (F-EXT-01)

Skills are recipes without a new binary, stored under `var/skills/<name>/`:

```text
skill.yaml      # name, description, when_to_use, tools[], required_caps[]
SKILL.md        # instructions injectées dans le prompt
```

Intents : `skill.create`, `skill.list`, `skill.get`/`describe`, `skill.activate`, `skill.uninstall`.
Governance: `skill.create` is not critical → Low refuses, Medium confirms, High auto.

### 7.6 Authoring modules by agents (F-EXT-03/04)

Two paths to an installable `.aospkg`:

1. **Script / ext-rt**: `module.scaffold` (kind=script) writes `handlers.yaml`; `module.package` copies the precompiled WASM `ext-rt` + converts the handlers to JSON.
2. **Rust→wasm32**: `module.scaffold` (kind=rust) + `module.compile` (critical cap, confirmation even in High) with static control (no `unsafe`, no `std::fs`/`net`/`process`) and `CARGO_NET_OFFLINE=true`.

`module.install` requires the `module.install` capability for an agent actor (humans exempt in v1). After install, the agent requests `tool.invoke:<name>` via `cap.request`.

The WASM ABI host additionally exposes: `web.search`, `web.browse`, `net.fetch`, `files.generate`, `mem.context` / `mem.user.*` / `mem.shared_*`, `ext.load_handlers`. Prohibited: `module.install`, `module.compile`, `secrets.get`, `agent.*`, `trust.set`.

---

## 8. UI subsystem

### 8.1 Components

- **Compositor** (display server minimal)
- **Shell direct** : dock/panels, file manager, settings, resource dashboard
- **Shell conversationnel** : timeline, streaming tokens, cartes d'actions
- **Transparency panel**: chain-of-custody of agent decisions (`agent.trace` timeline, aggregated sources, simple/complex badge). **Preview 0.1.2** ships this in egui (Agents tab + chat agent cards).
- **Control bar**: pause / stop / steer / retry on selected agent

### 8.2 Contraintes perf

- The UI runs on **interactive** priority > batch agents
- Inference should not block the UI thread (separate processes + IPC)
- Reserved UI memory budget (cannot be avoided by Placement Manager except critical)

### 8.3 Accessibility (F-UI-08)

- Compositor exposes a native accessibility tree (roles, labels) consumable by screen reader, independent of graphics rendering
- Full keyboard navigation on both shells (direct + conversational); no focus trap
- Contrast and text size configurable globally (system theme), respected by the modules via the `declarative_ui` mode (§7.3)
- `sandboxed_webview` mode (free HTML content of a module) is **not guaranteed** accessible by the system — recommendation to module developers rather than platform guarantee

---

## 9. Security

### 9.1 Trust boundaries

1. Microkernel TCB  
2. Signed system services
3. Modules WASM  
4. Agents  
5. Remote backends (untrusted)

### 9.2 Secrets

- `secrets` service: encrypted storage (hardware key if available, otherwise derived envelope key)
- agents receive **usage caps** (e.g. sign a request) not raw secrecy, except for admin exceptions

### 9.3 Audit

- signed append-only log
- events: grant/revoke cap, infer start/end, tool invoke, fs tx, policy deny
- not writeable to application agents

### 9.4 Politiques

Policy engine (simple declarative language):

```yaml
rule: deny_remote_secret_data
match:
  data_class: secret
  backend.privacy_class: remote
effect: deny
```

Effets possibles (`effect`) :

| Effet | Comportement |
|-------|--------------|
| `allow` | Action executed normally |
| `deny` | Action refused, `PermissionDenied` audited (§16) |
| `require_confirmation` | Action **suspended**; prompt sent to the Control bar (§8.1) with context (agent, action, data concerned); configurable timeout → refusal by default if no response (fail-closed) |

Example meeting F-SEC-07:

```yaml
rule: confirm_sensitive_side_effect
match:
  action.kind: [fs.delete, network.send_external, payment]
effect: require_confirmation
timeout_sec: 120
default_on_timeout: deny
```

The `require_confirmation` flow is implemented as an extension of the intent protocol (§2.4): the Semantic IPC Bus returns a status `pending_confirmation` with a `confirmation_id`; the agent remains suspended (`agent.pause` implied) until resolved.

### 9.5 Egress Network and Control (F-SEC-08)

Any outgoing network connection initiated by an agent or module goes through a dedicated capacity:

```text
cap://net/connect/<host-pattern>:<port-range>
```

Default rules:
- **Deny by default**: a module without explicit network capability cannot resolve DNS or open a socket.
- The network capabilities of **remote model backends** (§3.3) are managed separately by the Model Subsystem (already covered by `privacy_class`/routing, §3.7) and do not require additional generic `net.connect` capability for the calling agent — only the Backend Manager holds the corresponding network capability.
- For **generic modules/tools** (e.g. web search, third-party API call), the `net.connect` capability is explicitly requested in the manifest (§7.3) and reviewed by the user during installation (like file permissions).
- “Strict offline” mode (global policy): cuts all active `net.connect` capabilities, including those already granted, without uninstalling modules (reinforces F-MDL-06).
- Any refused connection attempt is audited (`policy deny`, §9.3) with the target host, for diagnosis.

Preview network tools (F-NET-01/02), all gated by the same egress control:

| Intent | Behaviour |
|--------|-----------|
| `web.search` | `engine`: `auto` (Brave API if `brave_search_api_key` → DuckDuckGo HTML → Bing HTML) or forced `brave` / `duckduckgo` / `bing` |
| `web.browse` | HTTP GET, HTML → plain text (no JS); `max_chars` (default 12_000) |
| `net.fetch` | Binary download into the logical FS (default `/downloads/`, `max_bytes`) |

User defaults live in `var/run/preferences.json` (`web_search_engine`, `web_browse_max_chars`, `web_fetch_max_bytes`, `network_online`).

---

## 10. Boot sequence (technical)

```text
1. Bootloader → microkernel
2. Init process (pid1-like) starts essential services:
   device-mgr, storage, audit, policy
3. Model Subsystem :
   a. scan /system/models
   b. verify signatures
   c. detect GPU/NPU
   d. place embedded-instruct + embedded-embed
   e. warm-up minimal (1 forward pass court)
4. Agent Runtime + Module Runtime
5. UI Shell + System Assistant agent
6. User session
```

If step 3 partially fails → degraded mode with clear messages; direct shell remains available.

> Step 6 (User session) includes onboarding during the very first startup (language, preferences, trust policies, cf. F-BOOT-03); target budget: NFR-07 (< 10 min offline, functional-specs.md §8).

### 10.1 System Updates (F-BOOT-06)

- System image in **double slot A/B**: update downloaded/applied to the inactive slot, activation at the next reboot.
- Health-check post-update (successful boot + essential services up within a limited time frame); failure → **automatic rollback** to the previous slot without user intervention.
- Distinct from user FS snapshots (§6.1): a system update never modifies `/home/<user>` nor `/var/agents`.
- Embedded models (§3.4) versioned independently of the system image to allow their updating alone (different size, release rate).

---

## 11. System APIs (excerpt)

### 11.1 Model API

| Method | Description |
|---------|-------------|
| `model.list` | List of registry models |
| `model.inspect` | Metadata + current placement |
| `model.load` | Charge with investment profile |
| `model.unload` | Frees resources |
| `model.set_placement` | Plan manuel / profil |
| `model.infer` | Streaming inference |
| `model.embed` | Embeddings |
| `model.cancel` | Annule job |
| `model.metrics` | Live metrics |

### 11.2 API Agent

| Method | Description |
|---------|-------------|
| `agent.create/start/pause/resume/kill` | Lifecycle |
| `agent.steer` | Runtime directive |
| `agent.grant/revoke` | Caps |
| `agent.state` | Cognitive + status |
| `agent.audit` | Action history |

### 11.3 FS / Mem / Mod

See dedicated sections; all exposed via Semantic IPC and SDK (languages: Rust, C ABI, future bindings).

### 11.4 Module API (F-MOD-01, F-MOD-03, F-MOD-06)

| Method | Description |
|---------|--------------|
| `module.list` | List of installed modules (Module Registry) |
| `module.search` | Local registry search (name, capacity provided) |
| `module.describe` | Complete manifesto + tool diagrams (introspection, F-MOD-03) |
| `module.install` | Installs from a `.aospkg` package, displays requested headings for review |
| `module.update` | Updates to a new version (caps diff displayed if change) |
| `module.uninstall` | Removes the module, revokes its active caps |
| `module.quarantine` | Isolate a module following repeated panic (§16) without uninstalling it |

### 11.5 Admin API (F-OBS-03)

| Method | Description |
|---------|--------------|
| `admin.policy.list/get/set` | CRUD on Policy Engine rules (§9.4) |
| `admin.quota.get/set` | Consultation/adjustment of quotas by agent or overall (§3.6.1) |
| `admin.models.set_default` | Sets the default template(s) for System Assistant and new agents |
| `admin.trust.get/reset` | Consultation and reset of a confidence score (§4.7) |
| `admin.audit.export` | Export of the audit trail for external analysis (F-OBS-04) |

> In v1 (single-user, §12), this API is not separated by a separate account: it is accessible from the direct UI under a “System Administration” area, protected by reinforced confirmation for high-impact actions (e.g. global policy modification).

---

## 12. Persisted data and schemas

```text
/system/
  models/           # embedded
  modules/          # system modules
  policies/
/var/
  models/           # user models
  modules/          # user-installed modules (Module Registry, §7.1)
  agents/           # states / snapshots
  memory/           # episodic stores
  audit/
  cache/weights/    # pages offload
/home/<user>/
  profile.yaml      # preferences, policies, Trust Manager settings
  documents/
  ...
```

Formats :
- weights : GGUF (v1 prioritaire), SafeTensors (roadmap)
- configs: signed YAML/JSON
- audit : JSONL append-only + checksum chain

**Profil utilisateur** (`/home/<user>/profile.yaml`) :

```yaml
user_id: local-primary
locale: fr-FR
role: admin              # v1 : toujours admin, mono-utilisateur (docs/functional-specs.md §2.3)
 policies_overrides: []   # references to admin.policy rules (§11.5)
model_defaults:
  assistant: local:embedded-instruct
  embed: local:embedded-embed
placement_profile_default: balanced   # §3.5.6
network_mode: online     # online | offline_strict (§9.5)
```

> Implementation note: the structure already provides for an explicit `user_id` and `role` so that future multi-user evolution (outside v1 scope) does not involve schema migration, only the addition of `/home/<user2>/` entries and a strict separation of capabilities between profiles.

---

## 13. Target Technical Metrics and SLOs (v1)

| Metric | Indicative target (machine ref. to freeze) |
|----------|----------------------------------------|
| Boot → wizard ready | < 30s SSD, warm embedded model |
| TTFT embedded-instruct (warm) | < 2s |
| UI input latency | < 100ms p95 |
| Degradation under disk offload | tok/s ≥ 25% of full-RAM on NVMe SSD |
| Insulation | kill -9 of an agent without kernel/UI impact |
| VRAM leak | 0 after unload + GC (documented fragmentation tolerance) |
| Competing agents (NFR-04) | ≥ 32 active lightweight agents (working memory + idle) without UI degradation |
| Concurrent Inference Flows (NFR-04) | ≥ 8 simultaneous flows via continuous batching (§3.6) without drop > 20% of the unit tok/s |

Reference machine (proposal):
- CPU 8 cores, 32 GB RAM, GPU 8 GB VRAM, SSD NVMe 512 GB

---

## 14. Proposed technological stack

| Domain | Proposed choice | Alternatives |
|---------|---------------|--------------|
| TCB language / services | **Rust** | C/C++ for isolated drivers |
| Kernel P1 | seL4 or Redox / custom Rust microkernel | Zircon concepts |
| Host de dev P0 | Linux + processes | QEMU microVM |
| Local inference | **llama.cpp** (via FFI) or **candle** | ggml, ort |
| Quantization formats | GGUF | GPTQ/AWQ later |
| WASM runtime | wasmtime or wasmi | |
| Index vectoriel | hnswlib-rs / usearch | sqlite-vss |
| UI | to decide (egui / iced / web composer sandbox) | |
| IPC serialization | CBOR or capnp | JSON (debug) |
| Build | cargo workspaces + image builder | |

> Portability (NFR-10): x86_64 is the primary development target (P0/P1). A best-effort aarch64 port (Apple Silicon in dev environment, edge/ARM server target) is followed in parallel from M1 to avoid late hardware abstraction debt, without blocking x86_64 milestones.

---

## 15. Interfaces with accelerator hardware

### 15.1 Device Manager GPU/NPU

- inventaire devices (PCI/virtio)
- memory budgets exposed to the Placement Manager
- streams de copie async (DMA)
- recovery on lost device (CPU fallback)

### 15.2 Abstraction `AccelDevice`

```text
trait AccelDevice {
  info() -> DeviceInfo
  alloc(size) -> MemHandle
  free(MemHandle)
  copy_host_to_device(src, dst, stream)
  copy_device_to_host(...)
  copy_device_to_device(...)
  sync(stream)
  // compute submitted through inference backend, not through this generic trait
}
```

Concrete backends (P0/P1): Vulkan compute / CUDA / Metal / CPU SIMD depending on platform.

---

## 16. Error handling (contracts)

| Situation | Comportement |
|-----------|--------------|
| OOM VRAM pendant load | replacer automatiquement ; sinon erreur `PlacementImpossible` + suggestion profil |
| Disque plein (weights cache) | purger cache cold ; sinon refuse load |
| Backend remote timeout | retry policy; local fallback if allowed; otherwise user error |
| Cap missing | `PermissionDenied` audited |
| Module panic | insulation; agent receives tool error; module can be quarantined |
| Corruption weights | hash fail → refuse load, notification |

---

## 17. Testing and validation

### 17.1 Niveaux

- unit : placement algorithm, cap attenuation, policy engine  
- integration: infer local with forced offload third party
- e2e: boot offline → onboarding → agent task → undo
- chaos : kill backends, pressure VRAM, disk full  
- security : sandbox escape attempts, cap forgery, audit tamper  

### 17.2 Mandatory placement scenarios

1. Model < VRAM → full GPU
2. VRAM < model < RAM → hybrid GPU+RAM
3. model > RAM → GPU+RAM+DISK streaming
4. 2 competing models → fair/priority eviction
5. Passing latency → hot memory-saver
6. High priority agent arrival during infer batch

---

## 18. Technical risks and mitigations

| Risque | Impact | Mitigation |
|--------|--------|------------|
| Microkernel complexity from scratch | deadline | P0 on Linux; P1 seL4/Redox |
| Disk offload performance too low | UX | Mandatory NVMe recommended; aggressive prefetch; quantize |
| Proprietary CUDA ecosystem | portability | abstraction AccelDevice; Vulkan/Metal |
| Data leak via remote | privacy | policy engine + local_only |
| Explosion complexity scheduler | stability | start FIFO+priorities; batching afterwards |
| Size image embedded models | distribution | strong quant; optional download larger models after boot |

---

## 19. Synthetic technical roadmap

> Summary view of milestones. For full details (deliverables, release gates, dependencies, risks by phase), see **`docs/development-plan.md`**.

### Milestone M0 — Spec & proto algorithms
- simulateur de Placement Manager
- registry + fake backends

### Milestone M1 — P0 userspace on Linux
- Real Model Subsystem (llama.cpp)
- functional RAM/disk offload
- agents multi-process + caps logiques
- UI minimale + dashboard ressources
- validation of aarch64 best-effort port in parallel (Apple Silicon dev / edge ARM), cf. §14

### Milestone M2 — WASM + Episodic Memory Modules
- package double-surface
- store vectoriel
- audit + undo FS

### Milestone M3 — Remote backends + routage privacy
- OpenAI-compatible
- policies

### Milestone M4 — Port microkernel / caps natives
- seL4 or equivalent
- Native semantic IPC

### Milestone M5 — GPU scheduler first-class + multi-GPU
- deep device-mgr integration
- continuous batching mature

---

## 20. Annexes

### A. Example of investment plan (32B Q6, 8GB GPU, 32GB RAM)

```text
Model: 24 GB weights, 80 layers
Plan (balanced):
  VRAM 7.5 GB : layers 0-18 + KV cache blocks (hot)
  RAM  14 GB  : layers 19-55
  DISK  rest  : layers 56-79 (prefetch window=2)
Estimated TTFT: ...
Estimated tok/s: ...
```

### B. Example infer flow with caps

```text
Agent A --(cap model.use:llama, cap fs.read:doc)--> model.infer
InferenceScheduler enqueue (prio=interactive)
PlacementManager ensure_residency(llama)
Backend.local.stream_tokens -> Agent A
Audit.append(infer_started/finished)
```

### C. Related documents

- `docs/functional-specs.md`
- `docs/vision.md`
- `docs/development-plan.md` — detailed plan by phase (deliverables, gates, risks)
- `adr/0002-model-placement.md` — placement algorithm and cost model (P0)
- `adr/0003-ui-framework.md` — choice of UI framework (accepted: egui)
- `adr/0005-offload-etat-de-l-art.md` — state of the art offload CPU/GPU/RAM/disk (pre-P1)
- (futur) `adr/0001-microkernel.md`
- (futur) `adr/0006-wasm-modules.md`
- (futur) `adr/0004-scope-mono-vs-multi-utilisateur.md`

---

## 21. Technical glossary

| Term | Definition |
|-------|------------|
| TCB | Trusted Computing Base — code whose correction is required for overall security (microkernel + signed services) |
| Shard | Placement unit (transformer layer, cache KV block, embedding table) handled by the Placement Manager |
| AccelDevice | Low-level abstraction trait for a GPU/NPU accelerator (alloc/copy/sync), independent of the inference backend |
| Continuous batching | Dynamically merging compatible inference queries into a single GPU batch, without waiting for current queries to complete |
| Pin | Opaque context attached to a capability, interpreted by the holder (e.g. distinguishing two uses of the same right) |
| Mitigation | Reduction of the rights of a capability during its derivation (`derive`) — never elevation |
| Weight Store | Model weight storage/mmap service, read-only, with page cache |
| Semantic IPC Bus | Typed message bus (intents + schemas + capabilities) above microkernel raw IPC |
| Token bucket | Continuously renewing quota mechanism used to limit inference consumption per agent (§3.6.1) |
| Level of confidence | Discrete level (`low`/`medium`/`high`) derived from the trust score, used by the Policy Engine to modulate the granting of capacities |
| Slot A/B | Double system image allowing updating with automatic rollback (§10.1) |
| Data class | Sensitivity label (`public`/`private`/`secret`) carried by an FS object (§6.4) |

## 22. Traceability matrix — functional requirements → technical components

> Objective: verify that no Must/Should functional requirement of `docs/functional-specs.md` remains without an identified technical component. Review done in v0.2.

| Functional block | IDs | Technical component(s) | Status |
|-------------------|-----|----------------------------|--------|
| Bootstrap | F-BOOT-01 → 06 | §10 Boot sequence, §10.1 System updates, §3.4 Embedded models | Covered |
| Agent Runtime | F-AGT-01 → 10 | §4 Runtime Agent (4.1 Execution Model, 4.2 Cognitive State, 4.3 Lifecycle, 4.5 System Assistant, 4.6 Supervisor, 4.7 Trust Manager) | Covered |
| Model management | F-MDL-01 → 10 | §3 Model Subsystem (3.2 Registry, 3.3 Backends, 3.6 Scheduler, 3.6.1 Quotas, 3.7 Routing) | Covered |
| Dispatch RAM/GPU/Disque | F-PLC-01 → 10 | §3.5 Placement Manager (3.5.1 → 3.5.8) | Couvert |
| Agentic memory | F-MEM-01 → 05 | §5 Memory Subsystem | Covered |
| Stockage / filesystem | F-FS-01 → 05 | §6 Storage Subsystem (6.1 Hierarchical FS, 6.2 Semantic index, 6.3 Undo, 6.4 Classification) | Couvert |
| Modules and applications | F-MOD-01 → 06 | §7 Module system (7.1 Components/Registry, 7.2 Format, 7.3 Manifesto, 7.4 Sandbox), §11.4 Module API | Covered |
| User interface | F-UI-01 → 08 | §8 UI subsystem (8.1 Components, 8.2 Perf constraints, 8.3 Accessibility) | Covered |
| Security / privacy / trust | F-SEC-01 → 08 | §9 Security (9.1 Trust boundaries, 9.2 Secrets, 9.3 Audit, 9.4 Policies + confirmation, 9.5 Network Egress) | Covered |
| Observability / administration | F-OBS-01 → 04 | §9.3 Audit, §11.5 API Admin, §13 Metrics, Metrics Exporter (§3.1) | Covered |
| NFR (performance, reliability, scalability, security, privacy, UX, extensibility, observability, portability) | NFR-01 → 10 | §13 Metrics and SLO, §14 Stack (portability note), §7.4 Sandbox (NFR-05), §9.4/9.5 (NFR-06) | Covered |

### Assumed deviations (outside v1 scope, voluntarily not covered technically)

| Sujet | Raison | Suivi |
|-------|--------|-------|
| Economic model / distributed marketplace | Out of scope v1 (local registry only) | `docs/vision.md` §Tracks to dig |
| Multi-user with strict account isolation | Outside scope v1 (single user, §12) | Future ADR `0004-scope-mono-vs-multi-utilisateur.md` |
| Energy/thermal management of the Placement Manager (mobile/edge) | Not priority as long as the main target is desktop/server | To be reassessed if porting aarch64 mobile (§14) advances |
