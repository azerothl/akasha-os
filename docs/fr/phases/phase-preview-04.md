# Phase P04 — Preview 0.4.0

**Langue :** [English](../../phases/phase-preview-04.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.4.0** : graphe mémoire typé + bootstrap enrichi,
coffre à secrets chiffré, revue de caps à l'install de modules, et hygiène
packaging pour artefacts CUDA **et** CPU. Pas seL4 / fer nu. Pas de canaux
chat. Pas de multi-GPU.

Priorités : [plan-evolutions.md](../plan-evolutions.md) Horizon B
(E6 / E7 / E10-lite). Séquençage : P04.1 → P04.2 ∥ P04.3 → P04.4 → P04.5.

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P04.1 | E6 store | Relations typées `similar` / `updates` / `supersedes` ; `mem.relate` / `neighbors` / `list` / `update` | fait |
| P04.2 | E6 UX | Bootstrap structuré ; onglet Mémoire lister / éditer / supprimer / superséder | fait |
| P04.3 | E7-lite | Vault chiffré (`vault.enc`), Settings secrets, `secret_name` modeld, MCP `${secret:…}` | fait |
| P04.4 | E10-lite | Revue de caps sur `module.install` (plus d'auto-approve) ; `share/mcp/servers.yaml.example` | fait |
| P04.5 | Docs / packaging | Docs de phase, FEATURES/TESTER/STATUS, sibling-bridge, `latest.json` 4 assets | fait |

Catalogue : [`docs/fr/FEATURES.md`](../FEATURES.md).

## Gates de sortie

| Gate | Critère |
|------|---------|
| P04.1 | Relier deux souvenirs `similar` ; `updates`/`supersedes` persistés après restart |
| P04.2 | Fait remplacé : recall + bootstrap voient le nouveau ; UI liste / édite / supprime |
| P04.3 | Clé Brave via Settings ; vault pas en clair ; un agent ne peut pas `secrets.get` |
| P04.4 | Install `.aospkg` → confirm revue de caps ; refus → quarantaine / caps vides |
| Régression | `cargo test --workspace` ; gates p4/p5 verts sur hôte CUDA |
| Packaging | 4 artefacts (Win/Linux × CUDA/CPU) + `latest.json` complet |

## Hors périmètre

E9 / P5.2 multi-GPU, marketplace public, fusion binaire sibling, ANN / éviction
F-MEM-04, canaux messagerie, computer-use, bare metal (E11–E13).

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

Tag `v0.4.0` seulement quand toutes les gates passent. La gate cohort PC reste
indépendante.
