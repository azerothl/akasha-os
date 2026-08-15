---
name: research
description: Web search and page fetch to document an answer
tools:
  - memory.recall
  - web.search
  - web.browse
  - net.fetch
  - memory.remember
  - docs.read
---
# Research

**Language:** English | [Français](../../docs/fr/skills/research/SKILL.md)

1. Clarify the question (current node / brief, not the whole goal if a plan exists).
2. `memory.recall` with that query before any external search.
3. `web.search` with a precise query (engine `auto` tries Brave→DuckDuckGo→Bing).
4. `web.browse` on 1–3 relevant URLs to read page text (prefer over `net.fetch` for HTML).
5. Synthesize and `memory.remember` key facts.
6. Cite sources in the final summary.

If search returns 0 results, retry with `engine: "bing"` or browse a known URL.
