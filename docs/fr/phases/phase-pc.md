# Phase PC — Preview cohorte

**Langue :** [English](../../phases/phase-pc.md) | Français


## Objectif

Distribuer **Agent OS Preview** à une cohorte : installer sur Windows /
Linux x64 (NVIDIA recommandé ; chemin CPU OK) ou macOS Apple Silicon,
tester depuis egui, renvoyer des feedbacks locaux. **Pas** seL4 / fer nu.

## Livrables

| # | Livrable | État |
|---|----------|------|
| PC.1 | `aos-session` | fait |
| PC.2 | Packaging Win/Linux/macOS | CI GitHub Actions + download modèles |
| PC.3 | UI egui cohorte | fait (+ tutoriel) |
| PC.4 | `feedback.submit` | local + issue GitHub |
| PC.5 | INSTALL + TESTER + FIRST-RUN | fait |
| PC.10 | Updates Releases non destructives | fait |
| PC.6 | Sessions chat parallèles persistées | fait |
| PC.7 | Mémoire session + user + panneau UI | fait |
| PC.8 | `web.search` + réseau opt-in | fait |
| PC.9 | FS binaire + `net.fetch` + `files.generate` | fait |
| PC.11 | Panneau transparence agent (`agent.trace`, sources, steer) | fait |
| PC.12 | Onglet Settings + `var/run/preferences.json` | fait |
| PC.13 | `web.search` multi-moteurs + `web.browse` | fait |
| PC.14 | Bootstrap mémoire + strip `<think>` Qwen | fait |

Catalogue : [`docs/fr/FEATURES.md`](../FEATURES.md).

## Gates PC.6–PC.9

| Gate | Critère |
|------|---------|
| PC.6 | 3 sessions concurrentes ; historique survit au restart |
| PC.7 | `mem.context` injecté dans infer ; remember / recall UI |
| PC.8 | recherche OK en online ; refus en `offline_strict` |
| PC.9 | download image + PDF généré sous `/downloads` |

Voir scénarios dans [`docs/TESTER.md`](docs/TESTER.md).

## Gate cohorte

3 Windows + 1 Linux + 1 macOS Apple Silicon suivent le
[chemin de 15 minutes](../TESTER.md#chemin-court-15-minutes) sans
toolchain ; ≥1 feedback chacun. Le protocole TESTER long reste la
checklist équipe (PC.6–PC.9, PC.11–PC.13).

```powershell
.\packaging\build-preview.ps1
```

## Suite

- Reste vague 2 : export zip, cancel infer/fetch, clipboard
- Cohorte pilote + triage feedback (gate encore ouverte)
- PV / fer nu restent parallèles (ADR 0001)
