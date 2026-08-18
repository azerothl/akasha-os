# Phase P08 — Preview 0.8.0

**Language:** English | [Français](../fr/phases/phase-preview-08.md)

## Goal

Ship **Akasha OS Preview 0.8.0**: local **image + audio (TTS)** generation
(E16); a **unified CPU/GPU host artefact** with UI + load-based device policy
(E17); **complete module uninstall** from the tester UI (F-MOD-01); an **E15
widget vocabulary expansion** so authored modules can build richer dashboards
without a webview; a **Providers** tab to add OpenAI-compatible cloud and
local servers (F-MDL-04); **one install command per OS**; plus a **cleanup /
refactor pass** before tag. Same bus, caps, audit, and Placement
Manager as GGUF chat. Not seL4 / bare metal. Not video. Not always-on voice.
Not a cloud-only sidecar. Not a new P6 gate number.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon B
(E16, E17) plus E15 follow-on and F-MDL-04. Sequencing: P08.1 → P08.2 ∥
P08.3 ∥ P08.5 ∥ P08.7 ∥ P08.11 ∥ P08.12 → P08.4 → P08.6 → P08.8 → P08.9 →
P08.10.

## Why this, not a hosted image API

egui stays the shell (ADR 0003). Diffusion and TTS **compete for VRAM** with
the loaded LLM (F-PLC-06 / F-PLC-09). Agents need an explicit `media.generate`
capability; outputs land under `/downloads` with an audit trail. A remote
OpenAI-compatible image endpoint may exist later as a *routed* backend
(`local_only` still wins for `secret` data) — it is not the default 0.8 path.

STT / 24/7 voice / chat channels stay with the sibling (anti-roadmap).

## Why unify CPU and GPU (E17)

Testers today download a **CUDA zip or a CPU zip**. Settings already expose
Inference **auto / gpu / cpu**, but it only applies on the **next full
boot**, and `auto` means “NVIDIA present?”, not machine load. A CUDA-linked
`aos-modeld` can fail to start without NVIDIA DLLs — that is why the split
exists.

Preview 0.8 ships **one artefact per OS**. `aos-session` probes hardware and
spawns a backend that is safe on that machine (CPU process has no CUDA DLL
dependency). Settings **gpu / cpu** restart `aos-modeld` in the current
session (cancel in-flight infer first; **seamless mid-token migrate is
P09 / E18**). **auto** is a Placement Manager
policy with hysteresis: GPU when VRAM allows; more RAM/CPU offload (or
cpu-only) under VRAM/CPU pressure or when E16 media needs the GPU; promote
back when pressure lifts. Pin `gpu` / `cpu` overrides auto.

`-CpuOnly` remains a **builder** escape hatch (no CUDA toolkit on the build
host), not a second tester download.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P08.1 | E16 registry | Image + TTS in the model catalogue / offerings; Placement Manager evicts LLM vs media shards | done |
| P08.2 | E16 image | `media.image.generate` (prompt → PNG under `/downloads`); cap review; audit | done |
| P08.3 | E16 audio | `media.audio.generate` (text → WAV/OGG TTS); same cap family; audit | done |
| P08.4 | E16 surface | Chat shows the image / plays the clip (E15 `image` / `audio` kinds land in P08.11) | done |
| P08.5 | E17 device | One Win + one Linux artefact; Settings gpu/cpu/auto apply without reinstall; auto follows VRAM/CPU load (hysteresis) | done |
| P08.6 | E16 packs | Optional media packs (download, not baked into the zip); same download fetches sd.cpp / piper into `bin/` if missing; GPU preferred for image | done |
| P08.7 | F-MOD-01 | Uninstall any non-bundled module from the UI; revoke caps; drop E15 tab; audit | done |
| P08.8 | Hygiene | Cleanup + refactor of Preview host crates (dead code, splits, naming); **no** behavior change | done |
| P08.9 | Install CLI | One documented command per OS: download + sha256 + overlay into the stable prefix | done |
| P08.10 | Docs / ship | Phase docs, FEATURES/STATUS/TESTER, version 0.8.0, site, packaging | done |
| P08.11 | E15 widgets | Closed vocabulary expansion: form JSON Schema + `select` / `radio` / `checkbox` / `textarea` / `bar_chart` / `image` / `audio` | done |
| P08.12 | F-MDL-04 | **Providers** tab: add / list / test / remove OpenAI-compatible cloud + local servers; models selectable in Chat | done |

Catalogue (once shipped): [`docs/FEATURES.md`](../FEATURES.md).

### Placement

Media models are evictable shards, not a second unmanaged GPU client.
If VRAM cannot hold LLM + diffusion: unload or refuse with an alternative
(smaller pack / CPU TTS / explicit skip). CPU TTS is in scope; CPU image
generation may be slow or skipped with a documented refuse.

### Module uninstall (P08.7 / F-MOD-01)

`module.uninstall` already exists on the bus and a Settings catalogue button
calls it, but it is not a complete tester path: only catalogue rows, no cap
revocation (the spec requires it), no confirm, and agent-created modules
(E15) are easy to miss.

0.8 ships uninstall as a first-class human surface:

- List **every** installed module (not only signed-catalogue entries)
- Confirm, then remove package dir + registry; **revoke** granted
  `tool.invoke:<name>` (and related) caps; audit the chain
- E15 sidebar tab and in-memory panel disappear immediately
- Bundled `notes` / `tasks` / `ext-rt` are **refused** (boot resync would
  restore them)
- Does **not** delete the module’s user documents under `/documents`
- Agents need `module.uninstall` + the same confirmation style as install

TESTER step: install a scaffolded module → uninstall → tab gone, caps gone,
re-install still works.

### E15 widget vocabulary (P08.11)

0.7’s closed tree (`column`, `row`, `heading`, `text`, `markdown`,
`stat_row`, `table`, `line_chart`, `form`, `button`) is too thin for real
module dashboards: `form` is text-only, there is no select/radio, and the
only chart is a line. Authors cannot invent kinds — unknown `kind` stays
**fail-closed**. 0.8 expands the **host** list, still closed.

Must:

- **`form` honors JSON Schema** of the bound tool: `string` → text,
  `integer`/`number` → numeric field, `boolean` → checkbox, `enum` →
  ComboBox; `format: textarea` (or long string) → multiline
- New kinds: `select`, `radio`, `checkbox`, `textarea`
- Charts: `bar_chart` (alongside existing `line_chart`)
- Media (shared with P08.4): `image`, `audio` — bind to a path or tool
  result under `/downloads`

Still fail-closed: unknown kinds refused, no partial tree, schema export
updated (`docs/bridge/aos-proto-decl-ui.json`). Scaffold default tree may
use a select + table when the primary tool has an `enum`.

Out of this pack: pie/scatter/canvas, date/color pickers, maps, rich-text
editor, Notes/Tasks rewrite onto E15, `sandboxed_webview`, third-party
kind plugins.

TESTER step: scaffold/install a script module whose UI uses `select` (or
an `enum` form field), `checkbox` or `radio`, and `bar_chart`; submit once;
result refreshes. Invalid kind still shows an error banner.

### Providers tab (P08.12 / F-MDL-04)

P3 already has `model.backend.add` and a single Settings “OpenAI / remote
key”. Testers cannot manage several endpoints, discover models, or point at
a local OpenAI-compatible server (Ollama / vLLM / LM Studio) without
editing YAML. 0.8 ships a first-class **Providers** sidebar tab.

Must:

- CRUD: add, edit, enable/disable, remove; persist under `var/providers/`
  (or equivalent); reload on boot
- Protocol: **OpenAI-compatible** `/v1/chat/completions` + optional
  `GET /v1/models` discovery (same client as P3.1)
- Presets (base URL + secret name, user can override):

  | Preset | Default endpoint | Secret |
  |--------|------------------|--------|
  | OpenAI | `https://api.openai.com/v1` | vault |
  | OpenRouter | `https://openrouter.ai/api/v1` | vault |
  | Anthropic | OpenAI-compat base (not native Messages API) | vault |
  | DeepSeek | `https://api.deepseek.com/v1` | vault |
  | z.ai | vendor OpenAI-compat base | vault |
  | Custom | user URL | optional vault |
  | Ollama | `http://127.0.0.1:11434/v1` | none |
  | vLLM | `http://127.0.0.1:8000/v1` | optional |
  | LM Studio | `http://127.0.0.1:1234/v1` | none |

- **Test** button: connectivity + list models (or a typed model id if
  discovery is empty)
- Chat / Models combo can select a provider model when routing allows
- Keys stay in the vault (never in the provider file, never to agents)
- `local_only` (default) ignores cloud providers; `balanced` / `remote_only`
  may use them; **`secret` data never leaves** (existing P3 rule)
- Loopback (`127.0.0.1` / `localhost`) counts as **local privacy** and does
  **not** require “Allow network”; WAN providers do
- Offline / no key / unreachable → clear error, local GGUF still works
- Audit: add / test / infer-via-provider

Out of this pack: native Anthropic Messages / Gemini / Bedrock protocols,
provider marketplace, making remote the default, shipping API keys.

TESTER step: add LM Studio or Ollama on loopback with `local_only` still
on → chat uses it; add a cloud preset with a vault key → refused until
Allow network + `balanced`; `secret` turn stays local.

### One-line install per OS (P08.9)

Today a tester must find the right GitHub Release zip, extract it, then run
`install.cmd` / `install.ps1` or `./install.sh` (and fight Windows
`ExecutionPolicy` on `.\install.ps1`). 0.8 publishes **one copy-paste
command per OS** on INSTALL, the site, and TESTER.

Intended surface (exact URLs frozen in P08.10):

```powershell
# Windows (PowerShell) — process-scoped Bypass, no local unsigned file
irm https://azerothl.github.io/akasha-os/install.ps1 | iex
```

```bash
# Linux x64
curl -fsSL https://azerothl.github.io/akasha-os/install.sh | sh
```

The hosted script: reads `latest.json` from GitHub Releases, picks the
**unified** Win or Linux artefact (E17), verifies sha256, extracts, runs
the existing non-destructive overlay into `%LOCALAPPDATA%\AgentOS-Preview`
or `~/.local/share/agentos-preview`, prints the launch command. HTTPS only;
print URL + hash before writing. Fail-closed on hash mismatch. No `cargo`.

Not winget / apt / Chocolatey. Not macOS. Authenticode/SmartScreen remains
a later publisher-cert issue (INSTALL already documents **More info → Run
anyway**).

### Cleanup / refactor (P08.8)

A dedicated pass **after** E16/E17/uninstall/P08.11/P08.12 land and **before** tag, so new
media, device, uninstall, widget, and provider code is included. Scope is the Preview host (`crates/aos-*`, egui, packaging scripts
touched by 0.8) — not a seL4 rewrite, not Notes/Tasks onto E15, not a new UI
toolkit.

In scope: dead code and unused deps; oversized modules split along existing
service boundaries; duplicated helpers collapsed; intent / cap / file naming
aligned with `media.*`; leftover bilingual UI labels that drifted. Gate:
behavior-preserving (`cargo test --workspace` + p4/p5 unchanged). Prefer
hygiene commits separate from E16 feature commits. Remainder after this
pass (egui `main.rs` / `aos-platformd.rs` still oversized; Scenarios tab
and chat role key `"vous"`) is **P09.7**, not abandoned.

## Exit gates

| Gate | Criterion |
|------|-----------|
| P08.1 | Catalogue lists at least one image pack and one TTS pack; loading media does not leak VRAM outside Placement Manager accounting |
| P08.2 | Prompt from chat or a module tool writes a PNG under `/downloads`; audited; agent without `media.generate` is refused |
| P08.3 | Text → playable audio file under `/downloads`; same cap / audit rules as image |
| P08.4 | Tester sees the image in chat and can play the clip without leaving Preview |
| P08.5 | Same zip boots on a machine without NVIDIA (CPU backend) and uses CUDA when present; Settings gpu/cpu take effect after modeld restart (no reinstall); auto demotes/promotes under load with hysteresis; pin overrides auto |
| P08.6 | CUDA-capable host can download media packs **and** their engines (`bin/sd` / `bin/piper`); CPU-only host still boots without them |
| P08.7 | Uninstall a non-bundled module from the UI (confirm); package gone; granted caps revoked (audited); E15 tab gone; `notes`/`tasks`/`ext-rt` refuse uninstall |
| P08.8 | Hygiene PR(s) land with no intentional behavior change; workspace tests + p4/p5 still green |
| P08.9 | On a clean Win and Linux machine, the documented one-liner downloads, verifies sha256, overlays the stable prefix, and Preview can launch (no zip hunt, no cargo) |
| P08.10 | FEATURES/STATUS/TESTER + version 0.8.0 |
| P08.11 | Installed module tab renders `select`/`radio`/`checkbox`/`textarea`/`bar_chart` (and `image`/`audio` if a bind exists); `form` fields follow JSON Schema types; unknown `kind` still refused (fail-closed, no partial tree) |
| P08.12 | Providers tab: add a loopback preset and a cloud preset; models appear in Chat; `local_only` blocks WAN; loopback works without Allow network; vault key never written to the provider file; `secret` infer stays local |
| Regression | `cargo test --workspace`; gates p4/p5 green on CUDA host |
| Packaging | Two tester artefacts (Win / Linux) + complete `latest.json` + hosted `install.ps1` / `install.sh` (`-CpuOnly` = builder-only) |

## Out of scope

Video generation, cloud image APIs as the default path, always-on microphone
/ STT, messaging channels, computer-use, `sandboxed_webview`, E13 compositor,
mid-token device hot-swap without cancel (**that is P09 / E18**), E7 TPM, live HTTP sibling daemon,
E9 / P5.2 multi-GPU, public marketplace, winget/apt/Chocolatey, PC cohort close, macOS, bare metal
(E11–E13), native Anthropic Messages / Gemini / Bedrock APIs (OpenAI-compat
presets only).

## Build

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda   # tester artefact (GPU+CPU backends)
.\packaging\build-preview.ps1 -SkipModels -CpuOnly       # builder-only, no CUDA toolkit
```

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh   # builder-only
```

## Next

Tag `v0.8.0` only when gates P08.1–P08.12 pass. PC cohort gate remains
independent. **Next: Preview 0.9.0 / E18 + E19** — mid-token device migrate without
cancel, plus extra image/TTS packs and a closed option schema, plus leftover
P08.8 host hygiene ([phase-preview-09.md](phase-preview-09.md) P09.7). After 0.9: remaining
Horizon B (E7 TPM, live HTTP adapter if a daemon is scheduled, E9 when a
second GPU exists).
