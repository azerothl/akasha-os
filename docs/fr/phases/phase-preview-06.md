# Phase P06 — Preview 0.6.0

**Langue :** [English](../../phases/phase-preview-06.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.6.0** : export de schémas du pont sibling +
contrat HTTP↔bus (E8), enveloppe keyring OS de la clé maître du vault
(E7, pas TPM), et catalogue local signé de modules (E10). Pas seL4 / fer nu.
Pas de canaux chat. Pas de multi-GPU. Pas de nouveau numéro de gate P6.
Pas de fusion binaire sibling.

Priorités : [plan-evolutions.md](../plan-evolutions.md) Horizon B
(E8 / E7-keyring / E10). Séquençage : P06.1 → P06.2 → P06.3 ∥ P06.4 →
P06.5 (stretch) → P06.6.

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P06.1 | E8 schémas | Export JSON Schema de `mem.*` + `secrets.*` sous `docs/bridge/` | fait |
| P06.2 | E8 contrat | Mapping HTTP JSON ↔ intents CBOR dans sibling-bridge (pas de daemon live) | fait |
| P06.3 | E7 keyring | Clé maître dans le keyring OS (CredMan / Secret Service) ; fallback fichier 0600 | fait |
| P06.4 | E10 catalogue | `share/modules/catalogue.yaml` signé ; vérif hash à l'install ; UI | fait |
| P06.5 | Stretch | Stop chat → `model.cancel` ; copie presse-papiers (message / Dépannage) | fait |
| P06.6 | Docs / ship | Docs de phase, FEATURES/STATUS/TESTER, version 0.6.0, site, packaging | fait |

Catalogue : [`docs/fr/FEATURES.md`](../FEATURES.md).

## Gates de sortie

| Gate | Critère |
|------|---------|
| P06.1 | `docs/bridge/` contient les schémas `mem.*` + `secrets.*` ; `cargo test` échoue si le dump diverge |
| P06.2 | sibling-bridge mappe HTTP JSON ↔ intents bus ; non-goals (fusion, canaux) inchangés |
| P06.3 | Après first-run, `master.key` n'est plus un fichier clair lisible ; secrets Settings survivent au restart ; Linux headless → fallback 0600 ; agent `secrets.get` refusé |
| P06.4 | Installer depuis le catalogue → revue caps + hash OK ; WASM altéré refusé ; refuse caps → quarantaine |
| P06.5 | Si livré : Stop interrompt un stream chat ; Copier met le texte du message dans le presse-papiers |
| Régression | `cargo test --workspace` ; gates p4/p5 verts sur hôte CUDA |
| Packaging | 4 artefacts (Win/Linux × CUDA/CPU) + `latest.json` complet |

## Hors périmètre

E7 TPM, daemon HTTP sibling live, fusion binaire, façade assistant-as-module,
E9 / P5.2 multi-GPU, marketplace public, canaux messagerie, computer-use,
bare metal (E11–E13), ANN / éviction F-MEM-04, fermeture cohort PC, macOS,
apply update automatique, export zip.

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

Tag `v0.6.0` seulement quand les gates P06.1–P06.4 et P06.6 passent. La gate
cohort PC reste indépendante. Après 0.6 : TPM plein si l'OS le justifie,
adaptateur HTTP live seulement si un daemon est planifié, E9 quand un 2e GPU
existe.
