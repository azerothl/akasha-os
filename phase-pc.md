# Phase PC — Preview cohorte

## Objectif

Distribuer **Agent OS Preview 0.1** à une cohorte : installer sur Windows /
Linux x64 + NVIDIA, tester depuis egui, renvoyer des feedbacks locaux.
**Pas** seL4 / fer nu.

## Livrables

| # | Livrable | État |
|---|----------|------|
| PC.1 | `aos-session` | fait |
| PC.2 | Packaging Win/Linux | CI GitHub Actions + download modèles |
| PC.3 | UI egui cohorte | fait (+ tutoriel) |
| PC.4 | `feedback.submit` | local + issue GitHub |
| PC.5 | INSTALL + TESTER + FIRST-RUN | fait |
| PC.10 | Updates Releases non destructives | fait |
| PC.6 | Sessions chat parallèles persistées | fait |
| PC.7 | Mémoire session + user + panneau UI | fait |
| PC.8 | `web.search` + réseau opt-in | fait |
| PC.9 | FS binaire + `net.fetch` + `files.generate` | fait |

## Gates PC.6–PC.9

| Gate | Critère |
|------|---------|
| PC.6 | 3 sessions concurrentes ; historique survit au restart |
| PC.7 | `mem.context` injecté dans infer ; remember / recall UI |
| PC.8 | recherche OK en online ; refus en `offline_strict` |
| PC.9 | download image + PDF généré sous `/downloads` |

Voir scénarios dans [`docs/TESTER.md`](docs/TESTER.md).

## Gate cohorte

3 Win + 1 Linux suivent `docs/TESTER.md` sans toolchain ; ≥1 feedback.

```powershell
.\packaging\build-preview.ps1
```

## Suite

- Vague 2 : citations, export zip, restore agent, cancel infer/fetch, clipboard
- Cohorte pilote + triage feedback
- PV / fer nu restent parallèles (ADR 0001)
