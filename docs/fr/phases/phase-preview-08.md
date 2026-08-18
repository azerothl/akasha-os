# Phase P08 — Preview 0.8.0

**Langue :** [English](../../phases/phase-preview-08.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.8.0** : **génération d’image + audio (TTS)**
(E16) ; un **artefact hôte CPU/GPU unifié** avec politique device UI + charge
(E17) ; plus une **passe de nettoyage / refacto** avant le tag. Même bus,
caps, audit et Placement Manager que le chat GGUF. Pas seL4 / fer nu. Pas de
vidéo. Pas de voix always-on. Pas un sidecar cloud-only. Pas de nouveau
numéro de gate P6.

Priorités : [plan-evolutions.md](../plan-evolutions.md) Horizon B (E16, E17).
Séquençage : P08.1 → P08.2 ∥ P08.3 ∥ P08.5 → P08.4 → P08.6 → P08.7 → P08.8.

## Pourquoi ça, pas une API image hébergée

egui reste le shell (ADR 0003). Diffusion et TTS **se disputent la VRAM**
avec le LLM chargé (F-PLC-06 / F-PLC-09). Les agents ont besoin d’une
capacité explicite `media.generate` ; les sorties atterrissent sous
`/downloads` avec une piste d’audit. Un backend distant compatible OpenAI
pour l’image pourra exister plus tard comme backend *routé* (`local_only`
gagne toujours pour les données `secret`) — ce n’est pas le chemin par
défaut en 0.8.

STT / voix 24/7 / canaux de chat restent au sibling (anti-roadmap).

## Pourquoi unifier CPU et GPU (E17)

Aujourd’hui le testeur télécharge un **zip CUDA ou un zip CPU**. Settings
expose déjà Inférence **auto / gpu / cpu**, mais ça ne s’applique qu’au
**prochain boot complet**, et `auto` veut dire « NVIDIA présent ? », pas la
charge. Un `aos-modeld` lié CUDA peut refuser de démarrer sans les DLL
NVIDIA — d’où le split.

Preview 0.8 livre **un artefact par OS**. `aos-session` sonde le hardware et
lance un backend sûr sur cette machine (le process CPU n’a pas de dépendance
DLL CUDA). Settings **gpu / cpu** redémarre `aos-modeld` dans la session
courante (annuler l’inférence en cours d’abord ; **la migration mid-token
sans cancel est P09 / E18**). **auto** est une politique
du Placement Manager avec hystérésis : GPU tant que la VRAM le permet ;
plus d’offload RAM/CPU (ou cpu-only) sous pression VRAM/CPU ou quand E16 a
besoin du GPU ; promotion inverse quand la pression retombe. Un pin
`gpu` / `cpu` surcharge auto.

`-CpuOnly` reste une échappatoire **builder** (pas de toolkit CUDA sur la
machine de build), pas un second téléchargement testeur.

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P08.1 | E16 registre | Image + TTS dans le catalogue / offerings ; le Placement Manager évince LLM vs shards média | prévu |
| P08.2 | E16 image | `media.image.generate` (prompt → PNG sous `/downloads`) ; revue de caps ; audit | prévu |
| P08.3 | E16 audio | `media.audio.generate` (texte → WAV/OGG TTS) ; même famille de caps ; audit | prévu |
| P08.4 | E16 surface | Le chat montre l’image / joue le clip ; kinds E15 `image` / `audio` optionnels | prévu |
| P08.5 | E17 device | Un artefact Win + un Linux ; Settings gpu/cpu/auto sans réinstall ; auto suit la charge VRAM/CPU (hystérésis) | prévu |
| P08.6 | E16 packs | Packs média optionnels (téléchargés, pas cuits dans le zip) ; GPU préféré pour l’image | prévu |
| P08.7 | Hygiène | Nettoyage + refacto des crates hôte Preview (code mort, découpes, nommage) ; **pas** de changement de comportement | prévu |
| P08.8 | Docs / ship | Docs de phase, FEATURES/STATUS/TESTER, version 0.8.0, site, packaging | prévu |

Catalogue (une fois livré) : [`docs/fr/FEATURES.md`](../FEATURES.md).

### Placement

Les modèles média sont des shards évincables, pas un second client GPU
non géré. Si la VRAM ne tient pas LLM + diffusion : décharger ou refuser
avec une alternative (pack plus petit / TTS CPU / skip explicite). Le TTS
CPU est dans le périmètre ; la génération d’image CPU peut être lente ou
sautée avec un refus documenté.

### Nettoyage / refacto (P08.7)

Passe dédiée **après** E16/E17 et **avant** le tag, pour inclure le nouveau
code média et device. Périmètre : hôte Preview (`crates/aos-*`, egui, scripts de packaging
touchés par 0.8) — pas une réécriture seL4, pas Notes/Tasks sur E15, pas un
nouveau toolkit UI.

Dans le périmètre : code mort et deps inutilisées ; modules trop gros
découpés selon les frontières de services déjà là ; helpers dupliqués
fusionnés ; nommage intents / caps / fichiers aligné sur `media.*` ; labels
UI bilingues qui ont dérivé. Gate : préservation du comportement
(`cargo test --workspace` + p4/p5 inchangés). Commits d’hygiène séparés des
commits fonctionnels E16.

## Gates de sortie

| Gate | Critère |
|------|---------|
| P08.1 | Le catalogue liste au moins un pack image et un pack TTS ; charger un média n’échappe pas à la comptabilité VRAM du Placement Manager |
| P08.2 | Un prompt depuis le chat ou un outil de module écrit un PNG sous `/downloads` ; audité ; agent sans `media.generate` refusé |
| P08.3 | Texte → fichier audio jouable sous `/downloads` ; mêmes règles de caps / audit que l’image |
| P08.4 | Le testeur voit l’image dans le chat et peut jouer le clip sans quitter Preview |
| P08.5 | Le même zip démarre sans NVIDIA (backend CPU) et utilise CUDA s’il est là ; Settings gpu/cpu après restart de modeld (pas de réinstall) ; auto rétrograde/promeut sous charge avec hystérésis ; le pin surcharge auto |
| P08.6 | Un hôte CUDA peut télécharger les packs média ; un hôte CPU-only démarre sans eux |
| P08.7 | PR(s) d’hygiène sans changement de comportement volontaire ; tests workspace + p4/p5 toujours verts |
| P08.8 | FEATURES/STATUS/TESTER + version 0.8.0 |
| Régression | `cargo test --workspace` ; gates p4/p5 verts sur hôte CUDA |
| Packaging | Deux artefacts testeur (Win / Linux) + `latest.json` complet (`-CpuOnly` = builder seulement) |

## Hors périmètre

Génération vidéo, APIs image cloud comme chemin par défaut, micro always-on
/ STT, canaux de messagerie, computer-use, `sandboxed_webview`, compositor
E13, hot-swap device en milieu de token sans cancel (**c’est P09 / E18**), E7 TPM, daemon HTTP sibling live, E9 / P5.2 multi-GPU, marketplace
public, fermeture cohort PC, macOS, fer nu (E11–E13).

## Build

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda   # artefact testeur (backends GPU+CPU)
.\packaging\build-preview.ps1 -SkipModels -CpuOnly       # builder seulement, pas de toolkit CUDA
```

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh   # builder seulement
```

## Suite

Tag `v0.8.0` seulement quand les gates P08.1–P08.8 passent. La gate cohort
PC reste indépendante. **Suite : Preview 0.9.0 / E18** — migration de device
en milieu de token sans cancel ([phase-preview-09.md](phase-preview-09.md)).
Après 0.9 : reste Horizon B (E7 TPM, adaptateur HTTP live si un daemon est
planifié, E9 quand un 2e GPU existe).
