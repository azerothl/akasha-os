---
name: morning-brief
description: Short local briefing from memory, open tasks, and notes — no network
license: MIT
when_to_use: >
  User asks for a morning briefing, daily recap, “what should I do today”,
  or to catch up after time away. Not for web research or a new project plan.
tools:
  - memory.recall
  - tasks.list
  - notes.list
  - notes.search
  - goal.complete
---
# Morning brief

**Language:** English | [Français](SKILL.fr.md)

A **local** briefing. Do not use the network.

1. `memory.recall` for durable preferences (UI language, what “today” means
   for this user). One or two narrow queries, not the whole memory dump.
2. `tasks.list` — open items only. Do **not** create or complete tasks.
3. `notes.list`, or `notes.search` with a short query (`today`, this week,
   or a preference from step 1). If the notes module is missing, skip and
   say so.
4. Write the briefing: **at most 8 short lines**. Facts first. Empty lists
   stay empty — do not invent work, events, or citations.
5. Do **not** call `web.search`, `web.browse`, or `net.fetch`.
6. Do **not** `notes.create` or `tasks.create` unless the user asked for
   that in this turn.
7. `goal.complete` with the briefing as the result.
