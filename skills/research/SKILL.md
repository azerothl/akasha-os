---
name: research
description: Web search and page fetch to document an answer
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

**Language:** English | [Français](../../docs/fr/skills/research/SKILL.md)

1. Clarify the question (current node / brief, not the whole goal if a plan exists).
2. `memory.recall` with that query before any external search.
3. `web.search` with **one topical query that names the subject** (e.g. `agentic OS` / `agentic operating system`). Never dictionary lookups of stopwords (`définition`, `qu'`, `ce`, `est`, `what`, `is`). Prefer the English jargon when the topic is English tech.
4. `web.browse` on 1–3 relevant URLs to read page text (prefer over `net.fetch` for HTML).
5. Synthesize and `memory.remember` key facts.
6. Cite only sources that support the claims: put `[1]`, `[2]`, … after each grounded fact, then a **Sources** list. Never invent URLs; never cite dictionary pages for an unrelated topic.

If search fails, try a simpler query or `web.browse` a known URL. Do not assume Bing/DuckDuckGo HTML will succeed.

`web.browse` does not run page JavaScript. Empty SPA shells are expected. If a browser MCP tool is already in your catalogue, use it for hydrated pages; otherwise cite the URL and do not invent the missing body.
