# Preview features — Akasha OS 0.6.0

**Language:** English | [Français](fr/FEATURES.md)

Catalogue of **shipped** Preview features on the Windows/Linux host.
This is **not** the bootable OS. Target v1 requirements live in
[functional-specs.md](functional-specs.md); phase gates in
[STATUS.md](STATUS.md).

> Date: 17/08/2026 · Preview **0.6.0**

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
| GGUF download | Offerings from `share/models/catalog-offerings.json` → `share/models/` |
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

- Unified local backends (llama.cpp CUDA **or** CPU) + optional remote OpenAI-compatible (P3)
- Routing: **local_only** (default) or **balanced** (Settings)
- Models tab: list / load / download offerings; set session default
- Live metrics: TTFT, tok/s, VRAM/RAM/disk (sidebar + Models)
- Green banner when newer packs fit the detected VRAM tier
- CLI: `aos-session --download-models <id>…`
- Continuous batching (`generate_batch`, `n_seq_max=8`) on CUDA hosts (P5.1)

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
| Modules | Local signed catalogue (E10); Install still requires cap review |
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
| Site | [azerothl.github.io/akasha-os](https://azerothl.github.io/akasha-os/) — Split-Flap Board, EN/FR |

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

## 11. Not in Preview 0.6.0

- Bootable / bare-metal image
- macOS
- Fully automatic update apply (download now, apply on next launch)
- Native audio / video generation
- Public module marketplace (local signed catalogue only)
- Messaging channels (Slack/Discord/etc.) in the OS core
- Simultaneous multi-user accounts
- Complete multi-GPU pipeline (P5.2; single-GPU hosts only)
- Live HTTP sibling daemon / TPM hardware envelope

Cohort protocol: [TESTER.md](TESTER.md).
