# Phase P05 — Preview 0.5.0

**Langue :** [English](../../phases/phase-preview-05.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.5.0** : extraction automatique **opt-in** des
faits durables issus des tours de chat vers la mémoire long terme
(`mem.user.remember` + E6 `updates`/`supersedes`). Pas seL4 / fer nu. Pas de
canaux chat. Pas de multi-GPU. Pas de nouveau numéro de gate P6.

Priorités : [plan-evolutions.md](../plan-evolutions.md) Horizon B
(**E14**). Séquençage : P05.1 → P05.2 → P05.3 → P05.4 → P05.5.

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P05.1 | E14 extract | Intent `mem.extract` ; prompt JSON ; filtre secrets ; `model.infer` basse priorité | fait |
| P05.2 | E14 persist | Remember + auto_link ; metadata `source=chat` ; dédup ~0.92 ; audit | fait |
| P05.3 | E14 UX | Toggle Settings (défaut ON) ; hook post-tour ; toast ; badge Memory `chat` | fait |
| P05.4 | E14 tests | Unitaires + intégration ; étape TESTER 7d | fait |
| P05.5 | Docs / ship | Docs de phase, FEATURES/STATUS/TESTER, version 0.5.0, site, packaging | fait |

Catalogue : [`docs/fr/FEATURES.md`](../FEATURES.md).

## Gates de sortie

| Gate | Critère |
|------|---------|
| P05.1 | `mem.extract` sur « je m’appelle X, je préfère le français » renvoie ≥1 fait JSON valide |
| P05.2 | Restart → recall / `mem.context` voient le fait ; 2e tour contradictoire crée `supersedes` |
| P05.3 | Défaut ON : toast + badge Memory après un tour durable. OFF : zéro écriture. Delete depuis l’UI retire le fait |
| P05.4 | Coller `sk-…` / `ghp_…` dans le chat → 0 remember ; audit `filtered` |
| Régression | `cargo test --workspace` ; gates p4/p5 verts sur hôte CUDA |
| Packaging | 4 artefacts (Win/Linux × CUDA/CPU) + `latest.json` complet |

## Hors périmètre

E7 TPM, fusion binaire sibling (E8), E9 / P5.2 multi-GPU, marketplace public,
canaux messagerie, computer-use, bare metal (E11–E13), file d’approbation
pré-store, extract depuis agents/MCP, ANN / éviction F-MEM-04, fermeture
cohort PC.

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

Tag `v0.5.0` seulement quand toutes les gates passent. La gate cohort PC reste
indépendante.
