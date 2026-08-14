---
name: research
description: Web search and page fetch to document an answer
tools:
  - web.search
  - web.browse
  - net.fetch
  - memory.remember
  - docs.read
---
# Research

**Language:** English | [Français](../../docs/fr/skills/research/SKILL.md)

1. Clarify the question.
2. `web.search` with a precise query (engine `auto` tries Brave→DuckDuckGo→Bing).
3. `web.browse` on 1–3 relevant URLs to read page text (prefer over `net.fetch` for HTML).
4. Synthesize and `memory.remember` key facts.
5. Cite sources in the final summary.

If search returns 0 results, retry with `engine: "bing"` or browse a known URL.
