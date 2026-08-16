# Phase P03 — Preview 0.3.0

**Langue :** [English](../../phases/phase-preview-03.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.3.0** : prouver la thèse OS dans l’UI (caps +
métriques d’inférence), élargir le cohort (chemin CPU-only), ajouter un
scheduler d’agents sous caps, et un second module WASM dual-surface
(**tasks**). Pas seL4 / fer nu. Pas de canaux chat.

Priorités : [plan-evolutions.md](../plan-evolutions.md) Horizon A (E1–E5).
Séquençage : E5 → E4 → E1 → E2 ∥ E3.

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P03.1 | E5 | Métriques GPU/modèles sidebar + onglet Models (TTFT, tok/s, VRAM) | fait |
| P03.2 | E4 | Surface caps : `cap.list`, revoke, audit | fait |
| P03.3 | E1 | Boot CPU-only + pack tiny + packaging CPU | fait |
| P03.4 | E2 | Intents `schedule.*`, persist, caps, UI minimale | fait |
| P03.5 | E3 | `tasks.aospkg` dual-surface + onglet Tasks | fait |

Catalogue : [`docs/fr/FEATURES.md`](../FEATURES.md).

## Gates de sortie

| Gate | Critère |
|------|---------|
| P03.1 | Après un infer, l’UI montre TTFT + tok/s + VRAM live pour le modèle chargé |
| P03.2 | Le testeur voit les caps d’un agent actif et peut révoquer une cap non critique (auditée) |
| P03.3 | Machine sans NVIDIA démarre la Preview ; chat local OK (lent acceptable) |
| P03.4 | Schedule 1 min fire un agent ; événement audit ; cancel stoppe les prochains fires |
| P03.5 | Humain crée une tâche en UI ; agent `tasks.list` la voit ; inverse aussi |
| Régression | `cargo test --workspace` ; gates p4/p5 verts sur hôte CUDA |
| Packaging | zip/tar CUDA + CPU publiables ; `latest.json` |

## Hors périmètre

E6–E13, canaux messagerie, marketplace public, computer-use, fusion binaire
sibling, multi-GPU P5.2, bare metal.

## Build

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda   # artefact GPU
.\packaging\build-preview.ps1 -SkipModels -CpuOnly       # artefact CPU
```

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh
```

## Suite

Tag `v0.3.0` seulement quand toutes les gates passent. La gate cohort PC reste indépendante.
