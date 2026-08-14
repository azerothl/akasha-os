---
name: file-author
description: Lire/écrire des fichiers et générer des artefacts (md, txt, json, csv, pdf)
tools:
  - fs.read
  - fs.write
  - fs.list
  - files.generate
  - docs.read
---
# File author

**Langue :** [English](../../../../skills/file-author/SKILL.md) | Français

Travaille sur le FS logique Agent OS.
- Lis avant d'écrire (`fs.read` / `docs.read`).
- Pour des artefacts utilisateur, préfère `files.generate` sous `/documents/` ou `/downloads/`.
- Garde les chemins stables et documente ce que tu as produit dans `goal.complete`.
