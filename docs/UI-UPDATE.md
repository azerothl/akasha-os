# Preview UI visual update

**Language:** English | [Français](fr/UI-UPDATE.md)

**Status:** ready-for-implement  
**Date:** 2026-08-31  
**Target:** Preview 0.15.x host app (`crates/aos-ui-egui`)

Visual implementation contract for the chamber-hybrid chrome pass. Product IA, a11y, and token tables stay in [UI.md](UI.md). This document owns **target look + mock references** until the pass lands and is merged back into `UI.md`.

> Scope: Preview host app display only. Not the marketing site, not seL4.

## Thesis

| Source | Keep |
|--------|------|
| Proposition **B** (refined plate) | Direct shell density, chat-first layout, thin instrument bezels |
| Proposition **C** (split chamber) | Chamber palette emphasis + live MEMORY / CAPS / GPU / AGENTS track graph |
| User constraint | Caps / memory / metrics **never** permanently steal chat width |

**Chamber drawer rule:** default **collapsed** (edge tab only). Expanded ≈ 30% right drawer with track graph. Grant/Deny lives on the drawer or a compact banner — never a permanent side column inside the chat stream.

### Anti-goals

- Permanent Caps/Memory column beside chat
- Mystic / bindu / gold / purple-glow chrome
- Soft marketing cards or rounded-full pills as primary containers
- Changing primary rail IA (Chat · Agents · Create · Memory) or moving tester tabs onto the rail

## Tokens

Reuse the chamber palette from [UI.md](UI.md) — void `#070b14`, ice-track `#5ee7ff`, signal `#2ef0c8`, hydrogen `#ff5a48`, paper `#e8eef6`. Do not invent a second app palette.

## Information architecture (unchanged)

| Layer | Items |
|-------|-------|
| Primary rail | Chat · Agents · Create · Memory |
| More | Notes · Library · Tasks · Models · Settings · Caps · Audit · Providers · DeclUI modules · Scenarios · Feedback |

Only **display** (density, chamber drawer, track graph, confirm plate) changes in this pass.

## Mock catalogue

Canonical assets live under [`docs/assets/ui-update/`](assets/ui-update/) (tracked in git). Local Impeccable explorations (`.impeccable/mocks/`, gitignored) are working scratch only — not the shared contract.

### Shared chrome + Chat

| Id | Surface | File | Acceptance |
|----|---------|------|------------|
| 00 | Shell rail + status | [`00-shell-rail.png`](assets/ui-update/00-shell-rail.png) | Left rail shows Chat/Agents/Create/Memory + More; bottom status bar (network, model, caps, language); no permanent right metrics column |
| 01 | Chat · chamber collapsed | [`01-chat-chamber-collapsed.png`](assets/ui-update/01-chat-chamber-collapsed.png) | Chat full width; Chamber edge tab only; conversation is the sole primary surface |
| 02 | Chat · chamber expanded | [`02-chat-chamber-expanded.png`](assets/ui-update/02-chat-chamber-expanded.png) | ~70/30 split; LIVE CLOUD CHAMBER with MEMORY/CAPS/GPU/AGENTS tracks; Hide/collapse affordance visible |
| 03 | Chat · Grant/Deny | [`03-chat-grant-deny.png`](assets/ui-update/03-chat-grant-deny.png) | Human sentence + technical detail + Grant/Deny; plate on drawer or compact banner, not full-screen takeover |

### Primary rail

| Id | Surface | File | Acceptance |
|----|---------|------|------------|
| 10 | Agents | [`10-agents.png`](assets/ui-update/10-agents.png) | Agent list / roster density matching B; Agents rail item active; chamber tab optional collapsed |
| 11 | Create (Image Studio) | [`11-create.png`](assets/ui-update/11-create.png) | Default studio: prompt, size, steps, generate, history — expert controls folded |
| 12 | Memory | [`12-memory.png`](assets/ui-update/12-memory.png) | Memory browse as full page (not a chat side column); Memory rail active |

### More (overflow)

| Id | Surface | File | Acceptance |
|----|---------|------|------------|
| 20 | Notes | [`20-notes.png`](assets/ui-update/20-notes.png) | Notes workspace under More; chamber palette; instrument density |
| 21 | Library | [`21-library.png`](assets/ui-update/21-library.png) | Library listing; no card-stack marketing look |
| 22 | Tasks | [`22-tasks.png`](assets/ui-update/22-tasks.png) | Task list / board density consistent with shell |
| 23 | Models | [`23-models.png`](assets/ui-update/23-models.png) | LLM catalog tab representative; local/honest labels |
| 24 | Settings | [`24-settings.png`](assets/ui-update/24-settings.png) | Me group visible (language, theme, scale); Models/Trust as peer groups |
| 25 | Caps | [`25-caps.png`](assets/ui-update/25-caps.png) | Caps as dedicated page; not duplicated as always-on chat column |
| 26 | Audit | [`26-audit.png`](assets/ui-update/26-audit.png) | Audit log / event list with ice-track hairlines |
| 27 | Providers | [`27-providers.png`](assets/ui-update/27-providers.png) | Backend/provider list; technical ids secondary |
| 28 | Scenarios | [`28-scenarios.png`](assets/ui-update/28-scenarios.png) | Tester cohort surface under More — not rail peer |
| 29 | Feedback | [`29-feedback.png`](assets/ui-update/29-feedback.png) | Tester feedback form under More |
| 30 | DeclUI module | [`30-module-declui.png`](assets/ui-update/30-module-declui.png) | Installed declarative module under More → Modules |

### Cross-cutting states

| Id | Surface | File | Acceptance |
|----|---------|------|------------|
| 40 | Empty chat | [`40-empty-chat.png`](assets/ui-update/40-empty-chat.png) | Empty session with composer; chamber collapsed; no fake content filler |
| 41 | First-run allowance | [`41-first-run-allowance.png`](assets/ui-update/41-first-run-allowance.png) | Post first chat: allowance recap; points to More → Scenarios for testers |
| 42 | Create expert fold | [`42-create-expert-fold.png`](assets/ui-update/42-create-expert-fold.png) | Expert mode open (sd.cpp / VRAM / advanced) without promoting those controls to the rail |

## Code mapping

| Id | `Tab` / surface | Render entry |
|----|-----------------|--------------|
| 00 | Shell | rail + status bar in `main.rs` |
| 01–03, 40 | `Tab::Chat` | `ui_chat` + future chamber drawer |
| 10 | `Tab::Agents` | `ui_agents` |
| 11, 42 | `Tab::Image` | Create / Image Studio |
| 12 | `Tab::Memory` | `ui_memory` |
| 20 | `Tab::Notes` | `ui_notes` |
| 21 | `Tab::Library` | `ui_library` |
| 22 | `Tab::Tasks` | `ui_tasks` |
| 23 | `Tab::Models` | `ui_models` / `models_page` |
| 24 | `Tab::Settings` | `ui_settings` |
| 25 | `Tab::Caps` | `ui_caps` |
| 26 | `Tab::Audit` | `ui_audit` |
| 27 | `Tab::Providers` | `ui_providers` |
| 28 | `Tab::Scenarios` | `ui_scenarios` |
| 29 | `Tab::Feedback` | `ui_feedback` |
| 30 | `Tab::Module(_)` | DeclUI host |
| 41 | First-run flow | onboarding / allowance recap |

Primary vs overflow helpers: [`crates/aos-ui-egui/src/nav.rs`](../crates/aos-ui-egui/src/nav.rs).

## Suggested implementation order

1. Shell tokens polish + Chat chamber drawer (collapsed / expanded / Grant-Deny) — mocks 00–03
2. Primary rail pages — 10–12
3. More workspace pages — 20–27, 30
4. Tester surfaces — 28–29
5. Empty / first-run / Create expert — 40–42

## Gate

**ready-for-implement** requires:

- [x] Thesis + anti-goals written
- [x] IA unchanged vs [UI.md](UI.md)
- [x] Every catalogue row has a named PNG under `docs/assets/ui-update/`
- [x] Every row has acceptance notes
- [x] Linked from [UI.md](UI.md)

Implementation of egui code is a **separate** plan after this gate.
