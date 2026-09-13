# Phase P19 — E23 runtime health plane

**Language:** English | [Français](../fr/phases/phase-preview-19.md)

## Goal

Ship **E23**: a four-layer runtime health plane (watchdogs already present,
ephemeral canary, SLO/EWMA, Isolation Forest residual + stderr clusters) without
a second always-on GGUF and without blocking boot.

Depends on Preview metrics (`model.metrics`), session watchdogs, notes module.
Not a new P6 gate. Not bare metal.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) **E23**.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P19.1 | E23 proto | `HealthSnapshot` / SLO / anomaly / clusters; intents `health.snapshot`, `health.canary` | done |
| P19.2 | E23 notes | `notes.delete` (file + mem + graph); canary title `__aos_canary__` | done |
| P19.3 | E23 runtime | `aos-platform` `health.rs` — EWMA 15 s, canary 5 min, Isolation Forest, embed clusters; spawn before `serve` | done |
| P19.4 | E23 UI | Audit Health panel; status bar on canary ok→fail; Troubleshoot consumes `health.snapshot` | done |
| P19.5 | E23 docs | Roadmap / FEATURES / STATUS / F-OBS EN+FR | done |

## Behaviour

```text
aos-platformd (before serve)
  ├─ SLO tick 15s → model.metrics + bus RTT → EWMA → IF residual (warning)
  └─ canary tick 5min
       ├─ sweep leftover __aos_canary__*
       ├─ model.list / agent.list / module.list / mem.stats
       ├─ notes.list → notes.create/read/delete (__aos_canary__)
       └─ tiny infer (max_tokens=4) if idle + Loaded; else skip
boot aos-session healthcheck ── lookup-only (unchanged)
```

- Isolation Forest **never** overrides `canary_ok`.
- No `mem.user.remember`, no `feedback.submit` / GitHub from the canary.
- Baseline invalidated when Preview `VERSION` changes.
- Budget target: &lt; 256 MiB RAM, 0 extra VRAM.

## Exit gates

- [x] Unit: EWMA / TTFT breach / skip-infer / IF spike / spawn &lt; 200 ms / proto roundtrip
- [x] Packaged notes verifies `notes.delete`
- [x] Docs EN+FR

## Out of scope

- Watchdogs for busd / capkd / agentd
- Prometheus / OTLP
- Media canary (packs optional; AK-001 stays in `media.rs`)
- Second critic GGUF
