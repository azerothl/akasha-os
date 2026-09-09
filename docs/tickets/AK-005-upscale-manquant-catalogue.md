# AK-005 — Offre upscale déclarée sans poids installé

- Priorité : P1
- Statut : corrigé, déployé dans le checkout — alias court ajouté
- Zone : packaging `share/models/upscale/` et catalogue des offres

## Reproduction

Une première commande utilisait l’identifiant catalogue `realesrgan-x4plus-anime` au lieu du nom de fichier attendu par l’API. L’API résout désormais cet alias (et ses variantes `local:`/underscore) vers `RealESRGAN_x4plus_anime_6B.pth` ; le nom canonique reste accepté.

## Impact

Le parcours UI et les clients externes peuvent utiliser soit l’identifiant court, soit le nom de fichier publié. La résolution reste limitée au répertoire `share/models/upscale/` et ne relâche pas les contrôles de chemin.

## Prise en charge proposée

Alias stable ajouté dans `resolve_media_asset`, avec test unitaire des formes courtes. La recette IPC avec `realesrgan-x4plus-anime` a produit `3048588` octets en `1927 ms`, image PNG 2048 × 2048 avec `engine=sdcpp`.
