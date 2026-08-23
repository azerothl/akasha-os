# Preview features — Akasha OS 0.11.0

**Language:** English | [Français](fr/FEATURES.md)

Catalogue of **shipped** Preview features on the Windows/Linux host.
This is **not** the bootable OS. Target v1 requirements live in
[functional-specs.md](functional-specs.md); phase gates in
[STATUS.md](STATUS.md).

> Date: 23/08/2026 · Preview **0.11.0**

### What's new in 0.11.0

- **E20 local decode**: KV cache **Q8_0** on GPU (F16 on CPU) via `LoadOptions`; Placement uses typed KV bytes
- **Prefix cache**: suffix prefill (`memory_seq_rm`) + warm `llama_state_*` — lower TTFT on turn 2 / E18 migrate restore (fail-closed)
- **Prompt-lookup speculative decode** on single-stream (C1) jobs only; batch N>1 stays the P5.1 path; exact sampler (reject → `memory_seq_rm`)
- **Metrics**: `draft_accept` / `prefix_hit` on the Models / sidebar line
- **E21** (same release): measured RAM + GPU/PCIe bandwidth in `hardware.json`; semantic prefix snap to ChatML turn/tool/think markers. MoE per-expert LRU is documented out of scope

### What's new in 0.10.1

- **E8 bridge parity**: `aos-bridged` exposes the full `mem.*` table live (plus secrets); binary ships in Preview `bin/`; smoke covers `mem.stats` / `mem.list`
- **Auto-download updates** (opt-in, Settings): when a newer Release is found, download into `var/updates/` in the background; banner shows “ready — relaunch to apply”; apply still on next launch
- **E15 widgets**: `pie` and `scatter` (host-rendered; still no webview)
- **img2img**: closed options `init_image` + `strength` on `media.image.generate`; Image studio checkbox + slider; inpaint/mask still out of scope

### What's new in 0.10.0

- **E7 TPM**: vault master key prefers a real host TPM seal when available (Windows Platform Crypto `NCrypt` / `TPM_RSA_SRK_SEAL_KEY`); Linux falls back to OS keyring then file until tpm2 seal is wired. Presence of a TPM device alone does not set `master.backend=tpm`. No PCR sealing
- **E8 live bridge**: optional separate binary `aos-bridged` — loopback HTTP `/v1` JSON↔CBOR to the intent bus (mem + secrets.list; secrets.get/set service-style only). Not inside `aos-session`
- **E9 multi-GPU path**: Placement / llama layer `tensor_split` plumbing; P5 gate **skips** on 1-GPU hosts (honest STATUS — hard-green needs a 2-GPU run)
- **Media/UX polish**: Image studio **composition** canvas (overlapping blocks → prompt injection), **upscale** (RealESRGAN / `media.image.upscale`), expert DiT knobs; Wan/LTX catalogue rows are **experimental** (not TESTER-required). No product video UI
- **Image history**: studio reloads prior PNG sidecars (`*.meta.json`) — prompt, enriched prompt, composition
- **Chat UX**: distinct user / assistant bubbles; clearer thread roles
- **Product RAG**: at boot, `aos-platformd` indexes `docs/FEATURES|STATUS|TESTER` (+ `fr/`) into `product:docs`; each `mem.context` retrieves top-k chunks (budget-capped) so the assistant answers UI / changelog questions without stuffing the full catalogue into the system prompt
- **Internal seL4**: tag `sel4-pv-0.10.0` + CI QEMU gate (`AOS_GATE_VM_PASS`) — not in the tester zip / `latest.json`

### What's new in 0.9.0

- **Mid-token migrate** (E18): Settings **auto / gpu / cpu** calls `model.migrate` — the live completion continues on the same stream (no Stop, no cancelled turn). On NVIDIA the **cpu** pin stays on the CUDA `aos-modeld` with `n_gpu_layers = 0`. Fail-closed fallback is the 0.8 cancel+restart path (audited)
- **Closed media options** (E19): `media.image.generate` / `media.audio.generate` take a `deny_unknown_fields` object (size, steps, CFG, seed, sampler, negative, Piper knobs). Unknown keys are refused and audited. `aos-sd` maps only allowlisted flags — never a free-form argv string
- Extra optional packs: Flux2-class, Ideogram4-class, Piper `en_GB`; `extra_files` for VAE / CLIP / T5 / LoRA; Download pulls sidecars
- Settings / Models pick the default image pack and Piper voice; `/image` and tools honor them after restart
- **Image studio** tab (prompt, size, steps, CFG, seed, sampler, catalogue style / LoRA / VAE); **Open in studio** on a chat PNG
- **TTS card** in chat: `/speak` opens voice + knobs then **Generate** (agents may still call the intent directly)

### What's new in 0.8.0

- **Image + TTS** (E16): `media.image.generate` / `media.audio.generate` write PNG/WAV under `/downloads`; cap `media.generate`; Placement Manager accounts for media VRAM
- Chat slash `/image` / `/speak` shows the PNG and plays the clip; E15 `image` / `audio` widgets bind the same paths
- Optional media packs (`local:sd-v1-5`, Piper `en_US` / `fr_FR`) — download from Models, **not** in the zip; first-run does not pull them. The same download fetches the sd.cpp / piper engine into `bin/` if it is missing
- **Unified host artefact** (E17): one Win zip + one Linux tarball; `aos-modeld` (CUDA) + `aos-modeld-cpu` inside; Settings **auto / gpu / cpu** (0.9 migrates in-process; 0.8 restarted modeld)
- **Module uninstall** (F-MOD-01): Settings lists installed modules; confirm; revoke `tool.invoke:<name>`; refuse bundled `notes` / `tasks` / `ext-rt` / `canvas`
- **E15 widgets**: typed `form` (JSON Schema), `select` / `radio` / `checkbox` / `textarea` / `bar_chart` / `image` / `audio`
- **Providers** tab (F-MDL-04): OpenAI-compat cloud + loopback (Ollama / vLLM / LM Studio); keys in the vault; Chat combo groups local vs provider; `local_only` still allows loopback
- One-liner install: `irm https://azerothl.github.io/akasha-os/install.ps1 | iex` / `curl -fsSL https://azerothl.github.io/akasha-os/install.sh | sh` (sha256 fail-closed + overlay)

### What's new in 0.7.0

- **Declarative module UI host** (E15): installed modules with `ui.mode=declarative_ui` get a dynamic sidebar tab — no webview, no new hardcoded egui tab per module
- Closed widget tree in `ui/index.html` (`type: declarative_ui`): `column`, `row`, `heading`, `text`, `markdown`, `stat_row`, `table`, `line_chart`, `bar_chart`, `pie`, `scatter`, `form`, `button`, `select` / `radio` / `checkbox` / `textarea`, `image`, `audio`
- **`module.ui`** intent: platformd validates the document (fail-closed); host binds tool results and routes button/form submits through the same cap review as `module.invoke`
- **`module.scaffold`** optional `ui` JSON; package/compile copy a real widget tree (default: heading + form + table on the primary tool)
- JSON Schema export: [`docs/bridge/aos-proto-decl-ui.json`](bridge/aos-proto-decl-ui.json)
- Notes and Tasks tabs stay hardcoded; `notes`, `tasks`, and `ext-rt` are excluded from dynamic module tabs
- Chat **« crée un module »** launches an agent (host fallback if the model dumps UI JSON instead of `agent.spawn`)
- Scenarios: launch an agent to scaffold / package / install a script module

### What's new in 0.6.0

- **Sibling bridge schemas** (E8): JSON Schema export of `mem.*` / `secrets.*` under [`docs/bridge/`](bridge/)
- HTTP JSON ↔ CBOR intent **contract** in [sibling-bridge.md](sibling-bridge.md) (no live daemon)
- **OS keyring** for the vault master key (E7): Windows Credential Manager / Linux Secret Service; file 0600 fallback (`AOS_SECRETS_FILE_KEY=1`)
- **Local signed module catalogue** (E10): `share/modules/catalogue.yaml` + ed25519; hash check on install; Settings list / Install
- Chat **Stop** cancels the in-flight `model.infer`; **Copy** on messages and Troubleshoot body

### What's new in 0.5.0

- **Auto-remember from chat** (E14): Settings toggle (**on** by default)
- After each chat turn, low-priority local `mem.extract` → durable facts
- Persist via `mem.user.remember` + E6 `updates`/`supersedes` auto-link
- Secret filter: API keys / tokens / IBAN-like never auto-stored (audited)
- Memory list shows a **`[chat]`** badge on extracted facts; toast on store
- Non-blocking (coalesced); skips if a previous extract is still running
- **`user.ask`**: mid-task question in the linked chat; agent waits (`Blocked`),
  resumes on the next reply (FIFO if several agents wait; 10 min timeout)

### What's new in 0.4.0

- Typed memory graph (`similar` / `updates` / `supersedes`) + auto-link on remember
- Memory tab: list / edit / delete / supersede; structured agent bootstrap
- Encrypted secrets vault (Settings); MCP `${secret:name}`
- Module install requires cap review (no auto-approve); refuse → quarantine
- `share/mcp/servers.yaml.example`; `latest.json` lists CUDA **and** CPU artefacts
- Sibling bridge contract doc ([sibling-bridge.md](sibling-bridge.md))

### What's new in 0.3.0

- Live inference metrics (TTFT, tok/s, VRAM) in sidebar + Models
- Caps tab: list by holder + revoke (audited)
- CPU-only boot + `cpu` first-run pack (NVIDIA optional)
- Agent scheduler (`schedule.*`) + Settings UI
- Dual-surface **tasks** module + Tasks tab
- CUDA and CPU packaging artefacts
- Agent context budget + `PromptTooLong` retry; loop guard on truncated JSON
- Chat: markdown for agent actions; slash `/` popup above input; themes
- Parallel spawn briefs kept short; notes create/update chunking guidance

### What's new in 0.2.0

- Public site rebuilt as a split-flap departure board (EN/FR)
- Packaged notes module resyncs on boot after a Preview update
- In-app **Troubleshoot** collects diagnostics and can open a GitHub report
- `notes.read` accepts `title`, `name`, `path`, or `slug`

---

## 1. First launch

| Feature | What it does |
|---------|----------------|
| NVIDIA optional | Without GPU, starts CPU-only (slow OK); `AOS_REQUIRE_GPU=1` or Settings→GPU to refuse |
| Disk checks | Refuses to start without enough free space |
| Hardware probe | Writes `var/run/hardware.json` (VRAM, RAM, disk); tier includes **cpu** |
| Model selection | Auto-best pack by tier (`cpu` / low / mid / high) |
| GGUF download | Offerings from `share/models/catalog-offerings.json` → `share/models/` ; media packs also fetch `bin/sd` / `bin/piper` |
| In-app tutorial | 4-step onboarding (language, trust, routing, scenarios) |
| Ordered boot | bus → capkd → auditd → modeld → platformd → agentd → egui |

Tiers:

- **cpu** (no NVIDIA): Qwen3.5-4B + Embedding 0.6B
- **low** (&lt;10 GiB VRAM): Qwen3.5-4B + Embedding 0.6B
- **mid** (10–20 GiB): Qwen3.5-9B + Embedding 0.6B
- **high** (≥20 GiB): Qwen3 30B-A3B + Embedding 0.6B

See [FIRST-RUN.md](FIRST-RUN.md).

---

## 2. Chat and sessions (PC.6)

- Parallel **persisted** conversations; history survives restart
- **Per-session model** combo (falls back to the default instruct model)
- Streaming replies from the local model (offline by default); **Stop** cancels infer; **Copy** on messages
- Chat **agent cards** when a background agent is attached (`/agent` or assistant delegation)
- `mem.context` injection before infer (session hits + user facts)
- **Room mode** (slice 1–2 backend, slice 3 UI): `ChatSessionMode::Room` extends the same
  `ChatSession` with in-app salon members and `speaker_id` on transcript lines.
  Slice 2 adds the backend **RoomConductor** in `aos-agentd` (`chat.session.room.turn` →
  `agent.room_conduct` / `agent.room_turn`). Slice 3 adds Chat UI under the existing Chat tab:
  members strip, **Enable room**, persona picker (Researcher / Critic / Coder / Planner),
  `@` roster autocomplete, roster-resolved bubble names, room thinking indicator + cancel,
  and Agents tab **Add to session** (+ optional join-on-create when a Room session is active).
  No Telegram/Discord channel — distinct from messaging channels
  (see [sibling-bridge.md](sibling-bridge.md)).

Slash commands:

| Command | Action |
|---------|--------|
| `/commands` | List commands |
| `/help` | System snapshot (services, agents, models, memory, audit) |
| `/agent <task>` | Start a background agent; card in the current session |
| `/notes` | List notes |
| `/notenew <title> \| <body>` | Create a note |
| `/notesearch <query>` | Semantic note search |
| `/audit [n]` | Last *n* audit events |
| `/kill <id>` / `/pause <id>` | Control an agent |
| `/image <prompt>` | Generate a PNG (`media.image.generate`) under `/downloads` |
| `/speak <text>` | Generate a WAV (`media.audio.generate`) under `/downloads` |
| `/canvas` | Toggle the shared vector drawing canvas for this session |

**Chat canvas (vector strokes, live)** — session-scoped drawing surface (human + agents). Bundled WASM module `canvas` exposes `canvas.stroke` / `rect` / `ellipse` / `erase` / `clear` / `undo` / `get` / `export`. Document persists as `var/chat/<id>/canvas.json` (not diffusion). Distinct from Image Studio composition blocks and `/image`.

---

## 3. Memory (PC.7 + P04.1/P04.2 + P05 / E14)

- Long-term **user facts**: remember / recall / list / edit / delete in the Memory tab
- Typed relations: `similar`, `updates`, `supersedes` (auto-link on close remembers)
- Session + user hits assembled as `mem.context` for chat and agents
- Agents also have episodic memory (`mem.episodic_*`) and `memory.remember` / `memory.recall`
- **Memory-first bootstrap**: `task.assess` then structured `mem.bootstrap` (active facts + similar neighbors; superseded omitted)
- **Auto-remember from chat** (Settings, **on** by default): after each chat turn,
  `mem.extract` proposes durable facts → `mem.user.remember` with `source=chat`;
  secrets filtered; Memory list shows a `[chat]` badge

---

## 4. Notes (P2.6)

- Dual-surface WASM module (`notes.aospkg`)
- Human UI: create / list / search
- Agent tools: `notes.create`, `notes.update`, `notes.search`, …
- Same data for humans and agents
- On boot, `share/modules/notes.aospkg` is copied to `var/modules/notes` when the manifest hash or WASM fingerprint differs (a Preview update must not keep a stale module)
- `notes.read` accepts `title`, `name`, `path`, or `slug`

---

## 4b. Tasks (P03.5 / E3)

- Dual-surface WASM module (`tasks.aospkg`)
- Human UI: **Tasks** tab — create / list / complete
- Agent tools: `tasks.create`, `tasks.list`, `tasks.update`, `tasks.complete`
- Same JSON store for humans and agents (`/documents/tasks/tasks.json`)
- Resyncs on boot like notes; skill `tasks` shipped under `share/skills/`

---

## 5. Agents

Observe / Think / Act loop with capability checks, confirmation, and audit.

| Feature | Detail |
|---------|--------|
| Goal loop | `goal.complete` / `goal.fail`; max steps and timeout (Settings defaults) |
| `task.assess` | Classifies the goal as **simple** or **complex**; complex activates the planner skill |
| Skills | Declarative recipes (`share/skills/`, overridable under `var/skills/`) |
| Tools | Native, WASM module, MCP, or runtime (plan / spawn / memory) |
| MCP | Optional stdio servers (`share/mcp/servers.yaml.example`) |
| Sub-agents | `agent.spawn` / `agent.await` with a narrow brief |
| `user.ask` | Pause and ask the user in the linked chat; reply routes via steer |
| Hot-grant | `cap.request` under trust + confirmation |
| Authoring | `skill.create`; scripted WASM via `ext-rt` (`module.scaffold` / `package`) |
| Qwen think | Hybrid `<think>` blocks stripped from prompts and from the UI |

Shipped skills: **notes-writer**, **research**, **file-author**, **planner**, **tasks**.

### Scheduler (P03.4 / E2)

- Intents `schedule.create` / `schedule.list` / `schedule.cancel` (system; not chat channels)
- Persist under `var/schedules/`; ticker fires agents on interval (min 30s)
- Settings UI: create / list / cancel

### Transparency panel (F-UI-04 / F-UI-05)

Agent **Detail** (from the Agents tab or a chat card):

- Live state, step *n/max*, tokens, duration, simple/complex badge
- Skills and MCP servers; parent / children
- **Sources** (web / document / fetch) with open-in-browser links
- Step timeline (`agent.trace`): action, args, tool result, tool kind (native / module / mcp / runtime)
- Controls: **Pause**, **Resume**, **Retry**, **Kill**, **Steer**

---

## 6. Models

- Unified local backends (llama.cpp CUDA **or** CPU) + named **Providers** (P08.12 / F-MDL-04)
- Routing: **local_only** (default) or **balanced** (Settings)
- Models tab: list / load / download offerings; set session default
- Live metrics: TTFT, tok/s, VRAM/RAM/disk (sidebar + Models)
- Green banner when newer packs fit the detected VRAM tier
- CLI: `aos-session --download-models <id>…`
- Continuous batching (`generate_batch`, `n_seq_max`) on CUDA hosts (P5.1)
- **E20 decode (0.11):** KV cache Q8_0 on GPU (F16 on CPU); prefix reuse +
  warm `llama_state_*` for lower TTFT on turn 2 / E18 migrate; prompt-lookup
  speculative decoding on **single-stream** jobs only (batch N>1 unchanged)
- Live metrics also show optional `draft` accept ratio and `prefix` hit tokens

### Image + TTS (E16)

- Intents `media.image.generate` / `media.audio.generate`; cap `media.generate`; audit; files under `/downloads`
- Optional packs (`local:sd-v1-5`, `local:piper-en-us`, `local:piper-fr-fr`) — **not** in the zip; first-run skips them
- **Download a pack also fetches the engine** (`bin/sd` / `bin/piper` + DLLs / `espeak-ng-data`) if it is missing; boot repairs the same gap
- Without weights **or** engine, Preview writes a visible stub PNG / short WAV so the pipeline stays testable
- Placement Manager treats media weights as evictable shards vs the loaded LLM

### Providers (F-MDL-04)

- **Providers** tab: add / list / test / remove OpenAI-compatible cloud and loopback (Ollama / vLLM / LM Studio)
- API keys live in the vault, never in the provider file
- Chat combo groups local vs provider; `local_only` still allows loopback; WAN needs **balanced** + Allow network
- Infer with `secret` data stays local

---

## 7. Network (opt-in)

Default mode is **`offline_strict`** (deny-by-default egress). Enable
**Allow network** in the sidebar or Settings.

| Intent | Role |
|--------|------|
| `web.search` | Multi-engine search: `auto` (Brave → DuckDuckGo → Bing), or force `brave` / `duckduckgo` / `bing` |
| `web.browse` | Fetch HTML → plain text (no JavaScript); `max_chars` configurable |
| `net.fetch` | Download a URL into the logical FS (default `/downloads/`) |
| `files.generate` | Write `md` / `txt` / `json` / `csv` / `png` / `pdf` |

Optional secrets (`var/secrets/keys.yaml`):

```yaml
keys:
  brave_search_api_key: "BSA..."
  github_token: "ghp_..."
```

Without a Brave key, `auto` falls through to DuckDuckGo then Bing HTML.

---

## 8. Settings

Persisted in `var/run/preferences.json` (migrated from `onboarding.json` if needed).

| Group | Options |
|-------|---------|
| General | Language **en** / **fr**; default trust **low** / **medium**; Inference **auto** / **gpu** / **cpu** |
| Models | Routing `local_only` / `balanced` |
| Network / Memory | Allow network (same as sidebar); **Auto-remember from chat** (E14, **on** by default) |
| Agents | Default model, max steps (1–128), timeout (60–86400 s) |
| Schedules | Interval agent fires (`schedule.*`) |
| Secrets | Brave / GitHub / OpenAI keys → encrypted vault; master key in OS keyring |
| Modules | Local signed catalogue (E10); Install still requires cap review; Uninstall non-bundled modules (not `notes` / `tasks` / `ext-rt` / `canvas`) |
| Web | Search engine, browse max chars, fetch max bytes |

---

## 9. Audit, policy, caps, feedback, updates

| Area | What you get |
|------|----------------|
| Audit | Append-only hashed journal; Audit tab; kill `aos-auditd` → supervisor restarts it |
| Caps | Caps tab: `cap.list` by holder; revoke (audited) |
| Confirmation | Blocking banner for sensitive actions; timeout = deny (fail-closed) |
| Feedback | Local `var/feedback/` + optional GitHub issue (security reports stay local) |
| Troubleshoot | In-app diagnostics (NVIDIA, home, logs); opens a GitHub report when findings exist |
| App updates | Banner when a newer GitHub Release exists; overlay `bin/` + `share/` without touching `var/` or overwriting `etc/*.yaml` |
| Site | [azerothl.github.io/akasha-os](https://azerothl.github.io/akasha-os/) — orrery, EN/FR |

---

## 10. Security primitives (host)

Already on the Preview host (P0–P4), not only “planned”:

- Logical then native capabilities (`aos-caps` / `aos-capkd`)
- Semantic IPC (CBOR, typed intents)
- WASM sandbox (wasmtime) + cap injection
- Declarative policy, egress deny-by-default, trust manager
- Isolated daemons + autonomous auditd

seL4 VM track (PV.1–PV.3) is separate: see [phases/phase-vm-sel4.md](phases/phase-vm-sel4.md).

---

## 11. Not in Preview 0.11 (still out after E20)

- Bootable / bare-metal image
- macOS
- Product video UI / STT / always-on voice (Wan/LTX catalogue rows are experimental only)
- Inpaint / mask as a first-class intent (img2img via `init_image`/`strength` ships in 0.10.1)
- Native Messages/Gemini/Bedrock APIs (OpenAI-compat Providers only)
- Public module marketplace (local signed catalogue only)
- Messaging channels (Slack/Discord/etc.) in the OS core
- Simultaneous multi-user accounts
- Multi-GPU **hard-green** without a documented 2-GPU run (code path + 1-GPU skip ship in 0.10)
- PCR / measured-boot vault sealing / attestation
- Sibling binary merge / assistant-as-module
- Sandboxed webview / HTML/JS module UI (E13 compositor)
- `webview` widget kind (pie/scatter ship in 0.10.1)
- Second draft GGUF / vLLM-in-TCB / DFlash2 (E20 uses prompt-lookup only)
- seL4 guest in the public Preview zip (internal `sel4-pv-*` only)

Cohort protocol: [TESTER.md](TESTER.md).
