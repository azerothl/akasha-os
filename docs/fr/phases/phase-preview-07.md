# Phase P07 — Preview 0.7.0

**Langue :** [English](../../phases/phase-preview-07.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.7.0** : une **UI de module déclarative rendue
par l’hôte** (E15). Un module installé peut ouvrir une vraie surface humaine
(formulaires, tables, stats, courbes) sans webview et sans nouvel onglet
egui codé dans le shell. Pas seL4 / fer nu. Pas de `sandboxed_webview`.
Pas le compositor E13. Pas de nouveau numéro de gate P6.

Priorités : [plan-evolutions.md](../plan-evolutions.md) Horizon B (E15).
Séquençage : P07.1 → P07.2 → P07.3 ∥ P07.4 → P07.5.

## Pourquoi ça, pas une webview

egui reste le shell (ADR 0003). Les modules décrivent un **arbre de widgets
fermé** dans `ui/index.html` (`type: declarative_ui`) ; l’hôte le peint avec
les widgets egui existants + `egui_plot`. Le WASM ne fait toujours que des
outils. Pas de TCB HTML/JS, pas de Chromium/WebView2.

E13 (compositor / webview optionnelle sur fer nu) reste Horizon C.

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P07.1 | E15 schéma | Vocabulaire de widgets fermé + JSON Schema ; kinds inconnus refusés | fait |
| P07.2 | E15 hôte | Onglet(s) egui générique(s) pour les modules `ui.mode=declarative_ui` | fait |
| P07.3 | E15 bind | L’hôte charge les données via les outils du module ; boutons/formulaires → `tool.invoke` (caps inchangées) | fait |
| P07.4 | E15 authoring | `module.scaffold` / `package` écrivent un vrai arbre ; un agent peut l’émettre | fait |
| P07.5 | Docs / ship | Docs de phase, FEATURES/STATUS/TESTER, version 0.7.0, site, packaging | fait |

Catalogue (une fois livré) : [`docs/fr/FEATURES.md`](../FEATURES.md).

### Vocabulaire de widgets (fermé)

`column`, `row`, `heading`, `text`, `markdown`, `stat_row`, `table`,
`line_chart`, `form` (champs depuis le schéma d’entrée d’un outil), `button`
(invoque un outil nommé). `poll_ms` optionnel à la racine pour un refresh
live via le scheduler/bus existant — stretch s’il glisse.

Les onglets Notes et Tasks restent **codés à la main** en 0.7 (pas de
réécriture). Les modules nouveaux et créés par un agent passent par l’hôte
générique.

## Gates de sortie

| Gate | Critère |
|------|---------|
| P07.1 | Schéma publié ; l’hôte refuse les kinds inconnus (fail-closed, audité) |
| P07.2 | Installer un module `declarative_ui` ajoute une surface humaine sans éditer les onglets de `aos-ui-egui` à la main |
| P07.3 | Une table/courbe à l’écran est liée au résultat d’un outil ; un bouton l’exécute sous la même revue de caps qu’aujourd’hui |
| P07.4 | `module.scaffold` + `package` (script) produit un arbre UI non-stub ; un agent peut créer un module dont l’onglet montre plus qu’un titre |
| P07.5 | FEATURES/STATUS/TESTER + version 0.7.0 |
| Régression | `cargo test --workspace` ; gates p4/p5 verts sur hôte CUDA |
| Packaging | 4 artefacts (Win/Linux × CUDA/CPU) + `latest.json` complet |

## Hors périmètre

`sandboxed_webview`, HTML/JS, CSS, vidéo, cartes tuilées, iced/tauri,
compositor E13, réécrire Notes/Tasks sur l’hôte générique, E7 TPM, daemon
HTTP sibling live, E9 / P5.2 multi-GPU, marketplace public, canaux
messagerie, computer-use, fer nu (E11–E13), fermeture cohort PC, macOS.

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

Tag `v0.7.0` seulement quand les gates P07.1–P07.5 passent. La gate cohort
PC reste indépendante. **Suite : Preview 0.8.0 / E16** — génération locale
d’image + audio (TTS) ([phase-preview-08.md](phase-preview-08.md)). Après 0.8 :
reste Horizon B (E7 TPM, adaptateur HTTP live si un daemon est planifié, E9
quand un 2e GPU existe).
