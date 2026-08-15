# First run — Akasha OS Preview

**Language:** English | [Français](fr/FIRST-RUN.md)

**This is not a bootable OS.** Preview runs on Windows or Linux x64 with an
**NVIDIA** GPU.

Full feature list: [FEATURES.md](FEATURES.md).

## Before you launch

1. Recent NVIDIA driver (`nvidia-smi -L` OK).
2. ~8 GB free disk (recommended mid-tier pack: 9B + embedding).
3. Install via `install.ps1` / `install.sh`, or run `bin/aos-session`.

## Hardware-aware model setup

On first launch (no `var/models/installed.json` yet):

1. `aos-session` probes GPU VRAM / RAM / disk → `var/run/hardware.json`.
2. It opens a **Model selection** window (auto-best for your tier):
   - **low** (&lt;10 GiB VRAM): Qwen3.5-4B + Embedding 0.6B
   - **mid** (10–20 GiB, e.g. RTX 4080 SUPER): Qwen3.5-9B + Embedding 0.6B
   - **high** (≥20 GiB): Qwen3 30B-A3B + Embedding 0.6B (optional alternatives listed)
3. Confirm (or pick alternatives / optional models) → GGUFs download into `share/models/`.
4. Services start; egui opens with the in-app tutorial.

Catalogue: `share/models/catalog-offerings.json`. Installed registry:
`var/models/installed.json`.

## What you can do

| Tab / surface | Usage |
|---------------|--------|
| Chat / Sessions | Parallel sessions; **per-session model**; slash commands (`/help`, `/agent`, `/notes`…) |
| Memory | Long-term facts (remember / recall); injected as `mem.context` |
| Notes | Human notes + via agent (WASM module) |
| Agents | Goal loop, skills, tools, MCP; **model** at create; **Detail** timeline |
| Models | List / load / download offerings; update banner when newer packs fit |
| Audit | Signed events; kill auditd (supervisor restarts it) |
| Network (sidebar) | Opt-in `web.search` / `web.browse` / `net.fetch` |
| Settings | Language, trust, routing, agent defaults, search engine |
| Feedback | GitHub issue on azerothl/akasha-os |
| Scenarios | Cohort protocol (see [TESTER.md](TESTER.md)) |

### Agent detail

Open **Détail** on an agent card or the Agents tab: live state, simple/complex
badge, sources, step timeline, Pause / Resume / Retry / Kill / Steer.

### Settings

Preferences persist in `var/run/preferences.json` (language **en** / **fr**,
routing, trust, default agent model, max steps, timeout, search engine,
browse/fetch limits).

## Model updates

If `catalog-offerings.json` recommends models you do not have yet, a green
banner appears → **Open Models** → **Download**. Restart Preview after
download so `etc/modeld.yaml` picks up new paths.

CLI: `aos-session --download-models <id>…`

## Network

Off by default (`offline_strict`). Enable **Allow network**, then search
(engine `auto` = Brave → DuckDuckGo → Bing) or **Browse** a URL (HTML → text).
Optional Brave key: `var/secrets/keys.yaml` → `brave_search_api_key`.

## Quick troubleshooting

- No GPU → install the NVIDIA driver (no CPU mode in 0.1).
- Setup cancelled → delete nothing; relaunch to reopen model selection.
- Daemon logs → `var/run/*.stderr.log` (Troubleshooting button).
