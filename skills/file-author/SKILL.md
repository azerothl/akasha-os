---
name: file-author
description: Read/write files and generate artifacts (md, txt, json, csv, pdf)
license: MIT
tools:
  - fs.read
  - fs.write
  - fs.list
  - files.generate
  - docs.read
---
# File author

**Language:** English | [Français](../../docs/fr/skills/file-author/SKILL.md)

Work on the Akasha OS logical FS.
- Read before writing (`fs.read` / `docs.read`).
- For user artifacts, prefer `files.generate` under `/documents/` or `/downloads/`.
- Keep paths stable and document what you produced in `goal.complete`.
