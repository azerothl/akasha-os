# Phase PC — Preview cohorte

## Objectif

Distribuer **Agent OS Preview 0.1** à une cohorte : installer sur Windows /
Linux x64 + NVIDIA, tester depuis egui, renvoyer des feedbacks locaux.
**Pas** seL4 / fer nu.

## Livrables

| # | Livrable | État |
|---|----------|------|
| PC.1 | `aos-session` | fait |
| PC.2 | Packaging Win/Linux | scripts `packaging/` |
| PC.3 | UI egui cohorte | fait |
| PC.4 | `feedback.submit` | fait |
| PC.5 | INSTALL + TESTER | fait |

## Gate

3 Win + 1 Linux suivent `docs/TESTER.md` sans toolchain ; ≥1 feedback.

```powershell
.\packaging\build-preview.ps1
```

## Suite

- Cohorte pilote + triage feedback
- PV / fer nu restent parallèles (ADR 0001)
