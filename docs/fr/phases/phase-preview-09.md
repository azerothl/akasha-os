# Phase P09 — Preview 0.9.0

**Langue :** [English](../../phases/phase-preview-09.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.9.0** : **migration de device en milieu de
token** (E18). Après E17 (0.8), Settings gpu/cpu/auto et l’auto selon la
charge peuvent déjà changer de device en **annulant** le `model.infer` en
cours et en redémarrant `aos-modeld`. 0.9 conserve le **même flux** : les
tokens déjà émis restent à l’écran ; la suite continue sur le nouveau
device (CPU ↔ GPU) sans cancel visible pour l’utilisateur.

Dépend de P08 / E17. Pas seL4 / fer nu. Pas de nouveau numéro de gate P6.

Priorités : [plan-evolutions.md](../plan-evolutions.md) Horizon B (E18).
Séquençage : P09.1 → P09.2 → P09.3.

## Pourquoi après 0.8

0.8 doit d’abord livrer un artefact unifié et un chemin cancel-then-reload
correct. Copier ou reconstituer l’état KV / sampler entre backends llama.cpp
pendant qu’un stream est ouvert est un changement fail-closed à part : si la
migration n’aboutit pas, repli sur le cancel+restart 0.8 (audité), jamais
de tokens dupliqués ou perdus en silence.

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P09.1 | E18 migrate | Le Placement Manager peut déplacer un infer actif CPU ↔ GPU sans abort du stream ; KV/état chez le Model Subsystem | prévu |
| P09.2 | E18 policy | Pin UI et `auto` (charge / pression VRAM E16) utilisent migrate si un stream est live ; cancel+restart 0.8 reste le fallback | prévu |
| P09.3 | Docs / ship | FEATURES/STATUS/TESTER, version 0.9.0, site, packaging | prévu |

Catalogue (une fois livré) : [`docs/fr/FEATURES.md`](../FEATURES.md).

## Gates de sortie

| Gate | Critère |
|------|---------|
| P09.1 | Démarrer une longue complétion GPU (ou CPU) ; changer de device en cours de stream ; la réponse continue sans Stop et sans tour « cancelled » tronqué |
| P09.2 | `auto` sous pression VRAM migre sans que le testeur appuie sur Stop ; pin gpu/cpu surcharge toujours ; migrate en échec → fallback 0.8 + audit |
| P09.3 | FEATURES/STATUS/TESTER + version 0.9.0 |
| Régression | `cargo test --workspace` ; gates p4/p5 verts sur hôte CUDA |
| Packaging | Deux artefacts testeur (Win / Linux) + `latest.json` complet |

## Hors périmètre

Génération vidéo, micro always-on / STT, E7 TPM, daemon HTTP sibling live,
E9 / P5.2 multi-GPU, compositor E13, fermeture cohort PC, macOS, fer nu
(E11–E13). Speculative decode / fusion multi-backend de tokens.

## Suite

Tag `v0.9.0` seulement quand les gates P09.1–P09.3 passent. Après 0.9 :
reste Horizon B (E7 TPM, adaptateur HTTP live si un daemon est planifié, E9
quand un 2e GPU existe).
