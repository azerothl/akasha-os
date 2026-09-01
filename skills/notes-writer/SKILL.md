---
name: notes-writer
description: Create, update, link, and explore markdown notes via the notes module
license: MIT
tools:
  - notes.create
  - notes.update
  - notes.list
  - notes.read
  - notes.search
  - notes.links
  - notes.related
---

# Notes writer

**Language:** English | [Français](../../docs/fr/skills/notes-writer/SKILL.md)

Use `notes.*` tools to manage notes under `/documents/notes/`.

## Writing

- Prefer `notes.create` with a clear title and a **short** structured outline (not a full long guide).
- For long content: `notes.create` (stub) then several `notes.update` calls, **section by section** (keep each `content` under ~1200 characters) so the JSON action is not truncated.
- Use `[[Other Note]]` wikilinks instead of duplicating content.
- Before creating, check `notes.list` or `notes.search` to avoid duplicates.
- To change an existing note, use `notes.update` (same title/path) — do not recreate.

## Research workflow

1. `notes.search { query }` — find candidate notes (semantic score).
2. `notes.read` on the best hit.
3. `notes.related { title|path, topic }` — get linked notes with relevance scores for the topic.
4. `notes.read` the highest-scoring related notes; follow `notes.links` if needed.

## Tips

- `notes.related` scores graph neighbors (outgoing + backlinks) against `topic`.
- Keep titles stable: rename (new slug) is not supported in v1.
