# Preview UI / UX

**Language:** English | [Français](fr/UI.md)

Durable product spec for the **Preview host app** (`crates/aos-ui-egui`). The public marketing site (`website/`) keeps the cloud-chamber design system in [DESIGN.md](DESIGN.md); this document governs in-app chrome, navigation, and copy on Windows/Linux Preview.

> Scope: Preview 0.11.x host shell. Not the bootable seL4 image.

## Goals

1. **Chat-first** — default home is a conversation, not a tester checklist.
2. **Progressive disclosure** — everyday tasks on the primary rail; power tools and cohort protocol under **More**.
3. **Honesty without noise** — Preview limits stay visible but do not dominate chrome.
4. **Human language first** — confirmations and primary labels explain intent; technical ids stay in tooltips or expert folds.
5. **Chamber palette everywhere** — reuse void / ice-track / signal / hydrogen / paper from the public site; no new brand colors.

## Information architecture

### Two-layer navigation

| Layer | Items | Notes |
|-------|-------|-------|
| **Primary rail** | Chat · Agents · Create · Memory | Always visible in the left rail. Keyboard: `Ctrl+1` … `Ctrl+4`. |
| **More (overflow)** | Notes · Tasks · Models · Settings · Caps · Audit · Providers · DeclUI modules · *(tester)* Scenarios · *(tester)* Feedback | Collapsible section. Tester-only surfaces are **not** peer tabs on the rail. |

**Chat sessions** live under the Chat tab — not a new rail tab. **Direct** mode is the default 1:1 assistant. **Room** mode is an in-app multi-agent salon (not Telegram/Discord messaging): enable it per session with **Activer le salon** / **Enable room**, add built-in personas (Researcher, Critic, Coder, Planner), and send messages that route through `chat.session.room.turn` (conductor in `aos-agentd`). The session header shows a **members strip** (roster display names). Bubble labels resolve `speaker_id` via the roster — never a free-text spoof field. Stable per-speaker colors derive from `speaker_id`. Background workers spawned outside the salon still show **AgentRef** cards; room member replies do not duplicate as cards (`origin: room`).

**Chat canvas** is a shared vector drawing surface on the same session (not Image Studio / not diffusion). Toggle with the session bar **Salon** / **Canvas** toggles (or `/canvas`). Bare « draw » / « dessine » routes to **Create** (Image Studio / pixels); vector canvas is used when the Canvas toggle is open or the message says « sur le canvas » / « au trait ». Humans draw with Select, pen, eraser, line, spline, path, rect, and ellipse; named layers sit above the board (hide / lock / opacity). Agents use `canvas.*` tools only while Canvas is open. Strokes appear live (optimistic human paint + ~200 ms poll for agent ops). Export PNG / SVG / JSON under `/downloads`, not `media.image.generate`. The bucket tool is omitted: flood-fill exists only on PNG raster of leftover `Fill` ops.

**Create** is the Image Studio tab (`Tab::Image`). Expert sd.cpp controls stay inside the studio behind **Expert mode** — they are not promoted to the rail.

DeclUI modules installed with `ui.mode=declarative_ui` appear under **More → Modules**, not as primary-rail peers.

### What moves out of the flat tab list

Before this spec, ~13 peer sidebar tabs treated Chat, Scenarios, Feedback, Caps, and Providers equally. After:

- **Rail** = daily driver (chat, agents, image, memory).
- **More** = workspace admin, trust, and extensions.
- **Scenarios + Feedback** = cohort / tester protocol ([TESTER.md](TESTER.md)); reachable from More, not the default post-tutorial destination.

## Design tokens (chamber)

Canonical palette (same as [DESIGN.md](DESIGN.md) and `website/styles.css`):

| Token | Hex | Role |
|-------|-----|------|
| `void` | `#070b14` | Night ground, primary text on light themes |
| `ice-track` | `#5ee7ff` | Identity / event traces (mark, audit hairlines, named tracks) |
| `signal` | `#2ef0c8` | Live chrome: selection, focus ring, active rail, links |
| `hydrogen` | `#ff5a48` | Spark, warnings, destructive emphasis, update CTA |
| `paper` | `#e8eef6` | Body type on void; light theme ground |
| `mute` | ~42% signal in void | Idle labels, secondary chrome |

### Theme mapping (egui `Visuals`)

| Theme | Ground | Type | Accent | Notes |
|-------|--------|------|--------|-------|
| **dark** (default) | void | paper | signal | Matches public site night ether. |
| **light** | paper | void | signal | Inverted plates; void type on paper meets AA for body copy. |
| **soft** | paper + 3% void mix | void @ 90% | signal @ 85% | Reduced contrast for long sessions; still chamber hues. |
| **high_contrast** | void | paper | signal + 2px focus | Paper on void ≥ 12:1; signal used for focus/selection, not sole state indicator. |

Do not introduce purple glow, card radii, or a separate “app brand” — square bezels, instrument density.

## OS chrome

### Persistent status bar (bottom)

Always visible; wired to existing prefs/runtime (no new backends):

| Segment | Source | Interaction |
|---------|--------|-------------|
| **Network** | `preferences.network_online` + runtime `NetSetMode` | Shows Offline / Online. Click toggles opt-in (same as former sidebar checkbox). |
| **Model** | Session default or first loaded model in `SystemMetrics` | Label only; opens Models on click. |
| **Capabilities** | Count from last `cap.list` for active holder (or `—` if not loaded) | Click opens Caps. |
| **Update** | `var/updates/pending.json` or `var/run/update_available.json` | Shows pending version or hides when none. |
| **Language** | `preferences.language` | Click toggles EN ↔ FR. |

### Top banner (reduced)

- **Preview honesty** — one weak line (`Preview {version} — host app, not bootable OS`). Not a full-width warning stripe.
- **Agent notices, update download row, confirmations** — stay here when active.
- Tutorial / Report / Troubleshooting — compact actions, not dominant.

### Confirmations

Order of presentation:

1. **Human sentence** — e.g. “The agent wants to delete a file.”
2. **Technical detail** — action id, target path (monospace, secondary).
3. **Buttons** — **Grant** / **Deny** (not Accept/Refuse only, not GRANT as the only label).

Rich OS-extension confirms (module.install, cap.request, …) keep the caps/manifest review callout.

## Accessibility

**Target:** [WCAG 2.2 Level AA](https://www.w3.org/TR/WCAG22/) for Preview chrome and primary flows.

| Requirement | Preview behavior |
|-------------|------------------|
| Contrast | High-contrast theme uses paper-on-void body text; state never encoded in color alone (icons/labels duplicate status). |
| Focus | 2px `signal` outline on interactive controls; visible keyboard focus in rail and More. |
| Keyboard | `Ctrl+1`…`Ctrl+4` primary rail; `Ctrl+K` opens a light go-to palette (tabs + slash hint). |
| Motion | Respect `prefers-reduced-motion` where animations exist (public site key wind); egui chrome stays static. |
| Scale | Interface scale preference in Settings → Me (`ui_scale_percent`: 90 / 100 / 110 / 125). Applied via egui `zoom_factor` across rail, status bar, and panels. |
| Language | EN/FR parity per [I18N.md](I18N.md). |

## Progressive disclosure

### Image Studio (Create)

Default surface: prompt, size, steps, generate, history.

Expert fold: sd.cpp backends, flow-shift, VRAM budget, upscale/img2img — unchanged capability, not in the rail.

### Settings

Three section headings in **More → Settings** (progressive disclosure for expert folds):

| Group | Contents |
|-------|----------|
| **Me** | Language, theme, interface scale, auto-download updates |
| **Models** | Inference mode, routing (human labels; technical ids in tooltips), default agent/image/audio models, links to Models and Providers tabs |
| **Trust** | Default agent trust, network opt-in, auto-remember; collapsible: agent limits, web tool caps, secrets vault, module catalogue, schedules |

Expert controls (web fetch limits, secrets, catalogue, schedules, image W/H/steps) stay available behind collapsed headers — not removed, not promoted to the same weight as language/theme.

## First-run

Sequence (replacing “finish tutorial → Scenarios tab”):

1. Language from OS locale when possible (`en` / `fr`), overridable on step 2.
2. One chat turn — user sends a message, assistant replies.
3. **Allowance recap** — show what the agent was permitted to do (memory write, tools used, caps referenced).
4. Point testers to **More → Scenarios** for the cohort protocol; do not make Scenarios the default home.

## Copy guidelines

| Avoid on primary surfaces | Prefer | Technical name lives in |
|---------------------------|--------|-------------------------|
| `local_only` | “Offline models only” | Settings tooltip |
| `holder` | “Subject” or “Agent” | Caps expert panel |
| `capkd` | “Capabilities service” | Tooltip |
| `TTFT` alone | “Time to first token” or hide on status bar | Models metrics row |

Tester jargon is fine inside Scenarios, Feedback, and audit export — not on the rail or status bar.

## Related docs

- [PRODUCT.md](PRODUCT.md) — positioning and brand commitments
- [DESIGN.md](DESIGN.md) — public-site cloud-chamber system
- [I18N.md](I18N.md) — EN/FR doc and UI language rules
- [FIRST-RUN.md](FIRST-RUN.md) — install and first launch
- [TESTER.md](TESTER.md) — cohort protocol (Scenarios / Feedback)

## Implementation slices

| Slice | Status |
|-------|--------|
| Spec (this doc) | done |
| Primary rail + More overflow | in progress |
| Status bar (network, model, caps, update, language) | in progress |
| Chamber token mapping for four themes | in progress |
| First-run → chat + allowance recap | done |
| Settings Me/Models/Trust grouping | done |
| Interface scale preference | done |
| Chat Room UI (slice 3) | done |
| Full WCAG audit | planned |
