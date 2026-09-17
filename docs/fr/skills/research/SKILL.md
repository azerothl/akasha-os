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
3. `web.search` avec **une requête thématique qui nomme le sujet** (ex. `agentic OS` / `agentic operating system`). Jamais un mot de dictionnaire (`définition`, `qu'`, `ce`, `est`). Préférer le jargon anglais si le sujet est technique EN.
4. `web.browse` sur 1–3 URLs pertinentes pour lire le texte (préférer à `net.fetch` pour le HTML).
5. Synthétise et `memory.remember` les faits clés.
6. Ne cite que les sources qui appuient les faits : `[1]`, `[2]`, … après chaque affirmation étayée, puis une liste **Sources**. N’invente jamais d’URL ; ne cite pas de pages dictionnaire hors-sujet.

Si la recherche échoue, réessaie avec une requête plus simple ou `web.browse` une URL connue. Ne suppose pas que Bing/DuckDuckGo HTML réussira.

`web.browse` n’exécute pas le JavaScript de la page. Un squelette SPA vide est attendu. Si un outil MCP navigateur est déjà dans ton catalogue, utilise-le pour les pages hydratées ; sinon cite l’URL et n’invente pas le corps manquant.
