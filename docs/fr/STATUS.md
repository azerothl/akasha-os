# État d'avancement

**Langue :** [English](../STATUS.md) | Français

Résumé des phases livrées. Détail : [plan-developpement-phases.md](plan-developpement-phases.md),
[phases/](phases/). Surface Preview : [FEATURES.md](FEATURES.md).

**En-tête :** P0 ✅ / P1 ✅ / P2 ✅ / P3 ✅ / P4 ✅ / PV.1–PV.3 ✅ / P5.1 ✅ / PC 🚧

**Preview :** 0.8.0 (18/08/2026) — image + TTS locaux (E16 ; les packs tirent
leurs moteurs), artefact CPU/GPU unifié (E17), pack widgets E15, onglet
Providers, désinstall complet, install one-liner. Pas un OS bootable. Gate
cohorte encore ouverte. **Suite :** Preview **0.9.0** (E18 + E19) —
migration de device mid-token sans cancel, plus d’autres modèles image/TTS
et un schéma d’options fermé ; reliquat d’hygiène P08.8 (P09.7).

## P08 — Preview 0.8.0 (E16 + E17 + widgets E15 + Providers) — fait

| # | Évolution | État |
|---|-----------|------|
| P08.1 | E16 registre média + shards Placement Manager | fait |
| P08.2 | E16 `media.image.generate` (PNG sous `/downloads`) | fait |
| P08.3 | E16 `media.audio.generate` (TTS) | fait |
| P08.4 | E16 surface chat (kinds `image`/`audio` en P08.11) | fait |
| P08.5 | E17 artefact unifié + politique device UI / charge | fait |
| P08.6 | E16 packs média optionnels (Download tire aussi sd.cpp / piper) | fait |
| P08.7 | F-MOD-01 désinstall module (UI, révocation caps, onglet E15) | fait |
| P08.8 | Nettoyage + refacto (hôte Preview ; pas de changement de comportement) | fait |
| P08.9 | Install en une ligne par OS (download + sha256 + overlay) | fait |
| P08.10 | Docs / packaging / version 0.8.0 | fait |
| P08.11 | Vocabulaire E15 (select/radio/checkbox/textarea/bar_chart/image/audio + forms typés) | fait |
| P08.12 | F-MDL-04 onglet Providers (cloud + serveurs locaux OpenAI-compat) | fait |

Détail : [phases/phase-preview-08.md](phases/phase-preview-08.md).

## P09 — Preview 0.9.0 (E18 + E19) — prévu

| # | Évolution | État |
|---|-----------|------|
| P09.1 | E18 migrate CPU ↔ GPU en milieu de token (le stream continue) | prévu |
| P09.2 | E18 UI/`auto` utilise migrate ; cancel+restart 0.8 = fallback | prévu |
| P09.3 | E19 schéma d’options fermé sur `media.*` + Settings (allowlist sd.cpp / Piper) | prévu |
| P09.4 | E19 packs optionnels extra (Flux2, Ideogram4, voix Piper extra) | prévu |
| P09.5 | E19 surface Models / Settings ; `/image` et TTS honorent le choix | prévu |
| P09.7 | Finir le reliquat P08.8 (découpes hôte restantes ; i18n restant ; clé de rôle chat) | prévu |
| P09.6 | Docs / packaging / version 0.9.0 | prévu |

Détail : [phases/phase-preview-09.md](phases/phase-preview-09.md).

## P07 — Preview 0.7.0 (E15) — fait

| # | Évolution | État |
|---|-----------|------|
| P07.1 | E15 schéma de widgets fermé (kinds inconnus refusés) | fait |
| P07.2 | E15 hôte d’onglet egui générique pour les modules `declarative_ui` | fait |
| P07.3 | E15 bind outils → table/courbe/formulaire ; actions → `tool.invoke` | fait |
| P07.4 | E15 scaffold/package écrit un vrai arbre de widgets | fait |
| P07.5 | Docs / packaging / version 0.7.0 | fait |

Détail : [phases/phase-preview-07.md](phases/phase-preview-07.md).

## P06 — Preview 0.6.0 (E8 / E7-keyring / E10)

| # | Évolution | État |
|---|-----------|------|
| P06.1 | E8 export JSON Schema `mem.*` / `secrets.*` | fait |
| P06.2 | E8 contrat HTTP↔bus (pas de daemon live) | fait |
| P06.3 | E7 enveloppe keyring OS de la clé maître | fait |
| P06.4 | E10 catalogue local signé de modules | fait |
| P06.5 | Stretch : Stop chat + presse-papiers | fait |
| P06.6 | Docs / packaging / version 0.6.0 | fait |

Détail : [phases/phase-preview-06.md](phases/phase-preview-06.md).

## P05 — Preview 0.5.0 (E14)

| # | Évolution | État |
|---|-----------|------|
| P05.1 | E14 `mem.extract` + filtre secrets | fait |
| P05.2 | E14 persist + auto_link + audit | fait |
| P05.3 | E14 Settings opt-in + toast + badge | fait |
| P05.4 | E14 tests + TESTER 7d | fait |
| P05.5 | Docs / packaging / version 0.5.0 | fait |

Détail : [phases/phase-preview-05.md](phases/phase-preview-05.md).

## P04 — Preview 0.4.0 (E6 / E7 / E10-lite)

| # | Évolution | État |
|---|-----------|------|
| P04.1 | E6 graphe mémoire typé | fait |
| P04.2 | E6 bootstrap + UI Mémoire | fait |
| P04.3 | E7-lite vault secrets | fait |
| P04.4 | E10-lite revue caps + exemple MCP | fait |
| P04.5 | Docs / packaging / sibling-bridge | fait |

Détail : [phases/phase-preview-04.md](phases/phase-preview-04.md).

## P03 — Preview 0.3.0 (E1–E5)

| # | Évolution | État |
|---|-----------|------|
| P03.1 | E5 métriques UI (TTFT / tok/s / VRAM) | fait |
| P03.2 | E4 UI caps (`cap.list` / revoke) | fait |
| P03.3 | E1 chemin CPU-only + packaging | fait |
| P03.4 | E2 scheduler agent | fait |
| P03.5 | E3 module tasks dual-surface | fait |

Détail : [phases/phase-preview-03.md](phases/phase-preview-03.md).

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
