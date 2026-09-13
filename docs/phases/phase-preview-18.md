# Phase P18 — E22 instincts in-session

**Language:** English | [Français](../fr/phases/phase-preview-18.md)

## Goal

Ship **E22**: extend Preview `skill.pass` so Create | Later can appear **in the
current thread** under context pressure (or after repeated steers), with bounded
instinct injection. Nightly `skill.pass` becomes catch-up only.

Depends on 0.15 `skill.pass`. Not a new P6 gate. Not bare metal.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) **E22**.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P18.1 | E22 surface | `SkillPassCandidate.surface_now` / `source_session_id`; `pending_surface_offer` skips 05:00 when live | done |
| P18.2 | E22 consider | Intent `skill.pass.consider` — session+steer heuristic, fire-once per session, anti-spam | done |
| P18.3 | E22 hooks | UI post-turn pressure; agent worker budget / overflow / steer≥2; room `run_infer` pressure | done |
| P18.4 | E22 instincts | `instincts.json` store; inject ≤3 at confidence ≥0.7; Create promotes via `mark_created_and_promote` | done |
| P18.5 | E22 prefs/docs | Pref `instincts_in_session` (default true); FEATURES / STATUS / roadmap EN+FR | done |

## Behaviour

```text
chat / agent turn / room infer
  └─ tokens ≥ 75% prompt_budget  OR  steer_count ≥ 2  OR  PromptTooLong
       └─ skill.pass.consider (once per session_id)
            ├─ pending surface_now → Create | Later in this thread
            └─ upsert instincts → bounded system-prompt inject
nightly skill.pass (02–04h) ── catch-up only (surface after 05:00 if missed)
```

- Never auto-writes `var/skills/`.
- Heuristic only (Jaccard / domain buckets / steers); no per-turn extract LLM.
- Pref off → no live consider / inject; nightly catch-up remains.

## Exit gates

| Gate | Criterion |
|------|-----------|
| Unit | soft pressure 75%, fire-once, `surface_now` ignores morning hour, steers in cluster |
| Unit | inject ≤3 / confidence floor; Create → skill + promote instinct |
| Unit | no double card same day for same `pattern_id` (night + live) |

## Out of scope

ECC Homunculus / continuous Haiku observer, auto project→global promotion,
GGUF fine-tune, replacing the morning catch-up card.
