# AK-009 — Pack LTX incomplet (sidecar LLM manquant)

- Priorité : P0 (fonction vidéo indisponible)
- Statut : corrigé, déployé et vérifié
- Zone : `share/models/catalog-offerings.json`, installation `share/models/`

## Constat

Le modèle principal `ltx-2.3-22b-dev-Q4_K_M.gguf` et ses VAE/connecteurs sont présents,
mais le sidecar déclaré `gemma-3-12b-it-qat-UD-Q4_K_XL.gguf` est absent (seul un fichier
`.partial` de 2,4 Go existe). Le lancement direct de `sd.cpp` échouait après initialisation
GPU avec des tenseurs de conditioner introuvables.

## Correction prise en charge

`aos-modeld` vérifie désormais les `extra_files` déclarés pour une requête vidéo avant de
lancer le moteur et renvoie une erreur explicite listant les fichiers manquants. Aucun
artefact vidéo n'est créé et le GPU n'est pas initialisé inutilement.

## Validation historique

Avant approvisionnement, la requête minimale LTX renvoyait en 1 ms :
`modèle vidéo incomplet : fichiers auxiliaires manquants (gemma-3-12b-it-qat-UD-Q4_K_XL.gguf)`.

## Validation finale

Le fichier complet a été repris (7 432 229 248 octets), vérifié par SHA-256
`da98f81c86916ed1c76b3eeda56b25cb7B8352B01093E2EDB8028110FE2CB53B` et référencé dans
le catalogue. Une génération LTX IPC 256×256, 9 frames, 1 étape produit
`/downloads/recette-20260909-video-ltx-real.webm` (111629 octets, signature EBML valide,
`engine=sdcpp`).
