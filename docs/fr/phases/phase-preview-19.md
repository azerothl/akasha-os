# Phase P19 — E23 plan de santé runtime

**Langue :** [English](../../phases/phase-preview-19.md) | Français

## Objectif

Livrer **E23** : plan de santé runtime en quatre couches (watchdogs déjà
présents, canary éphémère, SLO/EWMA, Isolation Forest résiduel + clusters
stderr) sans second GGUF always-on et sans bloquer le boot.

Dépend des métriques Preview (`model.metrics`), des watchdogs session, du
module notes. Pas une nouvelle gate P6. Pas bare metal.

Priorités : [plan-evolutions.md](../plan-evolutions.md) **E23**.

## Livrables

| # | Évolution | Livrable | Statut |
|---|-----------|----------|--------|
| P19.1 | E23 proto | `HealthSnapshot` / SLO / anomalie / clusters ; intents `health.snapshot`, `health.canary` | fait |
| P19.2 | E23 notes | `notes.delete` (fichier + mem + graphe) ; titre canary `__aos_canary__` | fait |
| P19.3 | E23 runtime | `aos-platform` `health.rs` — EWMA 15 s, canary 5 min, Isolation Forest, clusters embed ; spawn avant `serve` | fait |
| P19.4 | E23 UI | Panneau Santé Audit ; barre de statut si canary ok→fail ; Dépannage consomme `health.snapshot` | fait |
| P19.5 | E23 docs | Roadmap / FEATURES / STATUS / F-OBS EN+FR | fait |

## Comportement

```text
aos-platformd (avant serve)
  ├─ tick SLO 15s → model.metrics + RTT bus → EWMA → résidu IF (warning)
  └─ tick canary 5min
       ├─ balayage __aos_canary__* orphelins
       ├─ model.list / agent.list / module.list / mem.stats
       ├─ notes.list → notes.create/read/delete (__aos_canary__)
       └─ tiny infer (max_tokens=4) si idle + Loaded ; sinon skip
boot aos-session healthcheck ── lookup-only (inchangé)
```

- L’Isolation Forest **ne bascule jamais** `canary_ok`.
- Pas de `mem.user.remember`, pas de `feedback.submit` / GitHub depuis le canary.
- Baseline invalidée si `VERSION` Preview change.
- Budget cible : &lt; 256 Mio RAM, 0 VRAM extra.

## Gates de sortie

- [x] Unit : EWMA / breach TTFT / skip-infer / spike IF / spawn &lt; 200 ms / roundtrip proto
- [x] Notes empaqueté vérifie `notes.delete`
- [x] Docs EN+FR

## Hors scope

- Watchdogs pour busd / capkd / agentd
- Prometheus / OTLP
- Canary média (packs optionnels ; AK-001 reste dans `media.rs`)
- Second GGUF critique
