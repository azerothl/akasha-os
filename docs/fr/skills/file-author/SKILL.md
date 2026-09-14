---
name: file-author
description: Lire/écrire des fichiers et générer des artefacts (md, txt, json, csv, pdf)
license: MIT
tools:
  - fs.read
  - fs.write
  - fs.list
  - files.generate
  - docs.read
---
# File author

**Langue :** [English](../../../../skills/file-author/SKILL.md) | Français

Travaille sur le FS logique Akasha OS.
- Lis avant d'écrire (`fs.read` / `docs.read`).
- Pour des artefacts utilisateur, préfère `files.generate` sous `/downloads/documents/` (ou `/documents/` si pertinent).
- Images / audio / vidéo : `/downloads/images|audio|video/` ; canvas : `/downloads/canvas/`.
- Garde les chemins stables et documente ce que tu as produit dans `goal.complete`.
