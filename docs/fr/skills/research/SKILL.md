---
name: research
description: Recherche web et récupération de pages pour documenter une réponse
license: MIT
tools:
  - memory.recall
  - web.search
  - web.browse
  - net.fetch
  - memory.remember
  - docs.read
---
# Research

**Langue :** [English](../../../../skills/research/SKILL.md) | Français

1. Clarifie la question (nœud / brief courant, pas le goal entier si un plan existe).
2. `memory.recall` avec cette requête avant toute recherche externe.
3. `web.search` avec une requête précise (`engine` auto : Brave→SearXNG→DuckDuckGo→Bing).
4. `web.browse` sur 1–3 URLs pertinentes pour lire le texte (préférer à `net.fetch` pour le HTML).
5. Synthétise et `memory.remember` les faits clés.
6. Cite les sources dans le résumé final.

Si la recherche échoue, réessaie avec une requête plus simple ou `web.browse` une URL connue. Ne suppose pas que Bing/DuckDuckGo HTML réussira.
