---
name: tasks
description: Create and manage shared tasks (human + agent dual-surface)
license: MIT
tools:
  - tasks.create
  - tasks.list
  - tasks.update
  - tasks.complete
  - goal.complete
---
# Tasks

Use the dual-surface **tasks** module so humans and agents share the same list.

1. `tasks.list` before inventing work that may already exist.
2. `tasks.create` with a clear `title` (optional `notes`).
3. `tasks.complete` when done; `tasks.update` to rename or edit notes.
4. Finish with `goal.complete` when the user goal is satisfied.
