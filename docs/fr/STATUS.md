# État d'avancement

**Langue :** [English](../STATUS.md) | Français

Résumé des phases livrées. Détail : [plan-developpement-phases.md](plan-developpement-phases.md),
[phases/](phases/). Surface Preview : [FEATURES.md](FEATURES.md).

**En-tête :** P0 ✅ / P1 ✅ / P2 ✅ / P3 ✅ / P4 ✅ / PV.1–PV.3 ✅ / P5.1 ✅ / PC 🚧

**Preview :** 0.1.2 (15/08/2026) — appli hôte Windows/Linux + NVIDIA ; pas un
OS bootable. Gate cohorte encore ouverte.

## P0 — Simulateur (validé)

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P0.1 | `crates/aos-placement` | Simulateur du Placement Manager (§3.5) |
| P0.2 | `crates/aos-caps` | Modèle de capacités logique (§2.3), 20 tests de sécurité |
| P0.3 | `crates/aos-registry` | Catalogue YAML + backends simulés |
| P0.4 | `crates/aos-sim` | 6 scénarios §17.2 + validation croisée llama.cpp |

## P1 — Model Subsystem réel (gate 6/6)

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P1.1–P1.3 | `aos-llama`, `aos-model` | Backend llama.cpp FFI (CUDA), placement, scheduler |
| P1.4 | `aos-agent` | Agent Runtime : workers isolés, caps, état cognitif |
| P1.5 | `aos-ipc` | Semantic IPC Bus v1 |
| P1.6 | `aos-ui` | TUI chat + dashboard |

## P2 — Modules WASM + mémoire + audit (gate 6/6)

| Livrable | Contenu |
|----------|---------|
| P2.1–P2.6 | wasmtime, mémoire, FS versionné, audit, module notes |

## P3 — Backends distants + sécurité (gate 4/4)

| Livrable | Contenu |
|----------|---------|
| P3.1–P3.6 | Remote OpenAI-compatible, policy, egress, confirm, trust, supervisor |

## P4 — Caps natives + isolation (gate 4/4)

Voir [phases/phase-p4.md](phases/phase-p4.md) et [ADR 0001](../../adr/0001-microkernel.md).

## PV — Piste VM seL4

Voir [phases/phase-vm-sel4.md](phases/phase-vm-sel4.md).

## P5.1 — Continuous batching

Voir [phases/phase-p5.md](phases/phase-p5.md).

## PC — Preview cohorte

| Livrable | État |
|----------|------|
| PC.1–PC.10 | Session, packaging, egui, feedback, docs, sessions, mémoire, search, fichiers, updates |
| PC.11 Transparence agent (timeline, sources, steer) | fait |
| PC.12 Settings + préférences persistées | fait |
| PC.13 Recherche multi-moteurs + `web.browse` | fait |
| PC.14 Bootstrap mémoire + strip think Qwen | fait |
| Setup modèles selon le matériel | fait |
| Site public (EN/FR) | fait |

Gate cohorte (3 Win + 1 Linux, sans toolchain) encore **ouverte**.

Détail : [phases/phase-pc.md](phases/phase-pc.md), [FEATURES.md](FEATURES.md).
Tables EN : [../STATUS.md](../STATUS.md).
