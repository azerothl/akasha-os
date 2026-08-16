---
name: notes-writer
description: Créer, mettre à jour, lier et explorer des notes markdown via le module notes
tools:
  - notes.create
  - notes.update
  - notes.list
  - notes.read
  - notes.search
  - notes.links
  - notes.related
---

# Rédacteur de notes

**Langue :** Français | [English](../../../skills/notes-writer/SKILL.md)

Utilise les outils `notes.*` pour gérer les notes sous `/documents/notes/`.

## Écriture

- Préfère `notes.create` avec un titre clair et un **outline court** (pas un guide entier).
- Contenu long : `notes.create` (stub) puis plusieurs `notes.update` **section par section** (garder chaque `content` sous ~1200 caractères) pour éviter la troncature JSON.
- Utilise des wikilinks `[[Autre note]]` plutôt que de dupliquer le contenu.
- Avant de créer, vérifie avec `notes.list` ou `notes.search` pour éviter les doublons.
- Pour modifier une note existante, utilise `notes.update` (même titre/chemin) — ne recrée pas.

## Workflow de recherche

1. `notes.search { query }` — trouver des notes candidates (score sémantique).
2. `notes.read` sur le meilleur hit.
3. `notes.related { title|path, topic }` — notes liées avec scores de pertinence sur le sujet.
4. `notes.read` les notes liées les mieux scorées ; suivre `notes.links` si besoin.

## Astuces

- `notes.related` score les voisins du graphe (sortants + backlinks) contre `topic`.
- Garde les titres stables : le rename (nouveau slug) n'est pas supporté en v1.
