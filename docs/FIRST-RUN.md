# First run — Akasha OS Preview 0.12.1

**Language:** English | [Français](fr/FIRST-RUN.md)

> Date: 25/08/2026 · Preview **0.12.1**

**This is not a bootable OS.** Preview 0.12.1 runs on Windows, Linux x64, or macOS Apple Silicon.
**NVIDIA is recommended on Win/Linux**; the same zip ships a CPU-linked `aos-modeld-cpu`
(Settings → Inference restarts modeld in-session). macOS builds are unsigned.

Full feature list: [FEATURES.md](FEATURES.md).

## Before you launch

1. NVIDIA driver (`nvidia-smi -L` OK), **or** Settings → Inference → CPU (same zip).
2. ~8 GB free disk (recommended mid-tier pack: 9B + embedding); less for the **cpu** pack.
3. Prefer `install.cmd` / `install.sh`, or run `bin/aos-session` (auto-syncs
   into `%LOCALAPPDATA%\AgentOS-Preview` / `~/.local/share/agentos-preview` so
   history, memory and notes survive version upgrades).
4. On Windows, if SmartScreen says **Unknown publisher**: **More info** →
   **Run anyway** (Preview is not code-signed yet).

## Hardware-aware model setup

On first launch (no `var/models/installed.json` yet):

1. `aos-session` probes GPU VRAM / RAM / disk → `var/run/hardware.json`.
2. It opens a **Model selection** window (auto-best for your tier):
   - **cpu** (no NVIDIA): Qwen3.5-4B + Embedding 0.6B
   - **low** (&lt;10 GiB VRAM): Qwen3.5-4B + Embedding 0.6B
   - **mid** (10–20 GiB, e.g. RTX 4080 SUPER): Qwen3.5-9B + Embedding 0.6B
   - **high** (≥20 GiB): Qwen3 30B-A3B + Embedding 0.6B (optional alternatives listed)
3. Confirm (or pick alternatives / optional models) → GGUFs download into `share/models/`.
4. Services start; egui opens with a short in-app tutorial (language → one chat turn → allowance recap).

Catalogue: `share/models/catalog-offerings.json`. Installed registry:
`var/models/installed.json`.

## What you can do

| Tab / surface | Usage |
|---------------|--------|
| Chat / Sessions | Parallel sessions; **per-session model**; slash (`/help`, `/agent`, `/image`, `/speak`…) |
| Memory | Long-term facts (remember / recall); injected as `mem.context`; optional auto-remember from chat (Settings) |
| Notes | Human notes + via agent (WASM module) |
| Agents | Goal loop, skills, tools, MCP; **model** at create; **Detail** timeline |
| Models | List / load / download offerings; optional image/TTS packs (not in the zip; Download also installs `bin/sd` / `bin/piper`) |
| Providers | OpenAI-compat cloud + loopback (Ollama / vLLM / LM Studio); keys in the vault |
| Audit | Signed events; kill auditd (supervisor restarts it) |
| Network (sidebar) | Opt-in `web.search` / `web.browse` / `net.fetch` |
| Settings | Language, trust, routing, agent defaults, search engine, auto-remember, gpu/cpu/auto |
| Feedback | GitHub issue on azerothl/akasha-os |
| Scenarios | Cohort protocol (see [TESTER.md](TESTER.md)) |

### Agent detail

Open **Detail** on an agent card or the Agents tab: live state, simple/complex
badge, sources, step timeline, Pause / Resume / Retry / Kill / Steer.

### Settings

Preferences persist in `var/run/preferences.json` (language **en** / **fr**,
routing, trust, default agent model, max steps, timeout, search engine,
browse/fetch limits).

## Model updates

If `catalog-offerings.json` recommends models you do not have yet, a green
banner appears → **Open Models** → **Download**. Restart Preview after
download so `etc/modeld.yaml` picks up new paths. A media pack download also
fetches the matching engine into `bin/` when it is missing.

CLI: `aos-session --download-models <id>…`

## Network

Off by default (`offline_strict`). Enable **Allow network**, then search
(engine `auto` = Brave → DuckDuckGo → Bing) or **Browse** a URL (HTML → text).
Optional Brave key: `var/secrets/keys.yaml` → `brave_search_api_key`.

## Quick troubleshooting

- No GPU → CPU-only mode starts automatically (slow OK), or install an NVIDIA driver / use the GPU package.
- Setup cancelled → delete nothing; relaunch to reopen model selection.
- Daemon logs → `var/run/*.stderr.log` (Troubleshooting button).
