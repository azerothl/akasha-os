# AK-015 — Vision Canvas incapable de lire un chemin logique

**Priorité :** P1  
**État :** Corrigé, déployé et vérifié

## Symptôme

Une requête `model.infer` avec un modèle Qwen3-VL chargé et une image générée dans
`/downloads/...` échouait avec `vision / mtmd: échec lecture image`. Le Canvas
utilise précisément ces chemins logiques pour son export live.

## Cause

Le dispatcher mtmd transmettait directement les chemins logiques au backend,
alors que celui-ci ouvre des chemins du système hôte. Les chemins absolus issus
du sélecteur de fichiers fonctionnaient déjà.

## Correction

`aos-model` résout désormais les chemins logiques existants sous
`$AOS_HOME/var/storage/data` avant de construire le job vision. Les chemins
non existants et les segments `..` restent inchangés, afin de ne pas élargir la
surface de fichiers autorisée.

## Vérification

- Test unitaire de résolution et de confinement ajouté.
- `cargo test -p aos-model --no-default-features --lib` : 29 tests OK.
- Preview redémarrée avec le binaire installé.
- `model.load local:qwen3-vl-4b` : `has_vision=true` après chargement du mmproj.
- `model.infer` avec `/downloads/recette-20260909-image.png` : réponse réelle
  `Chaleur.` en 3,37 s, sans erreur mtmd.
