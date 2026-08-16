---
name: planner
description: Break a complex goal into subtasks and delegate to sub-agents
tools:
  - plan.update
  - agent.spawn
  - agent.await
  - memory.remember
  - memory.recall
  - goal.complete
---
# Planner

**Language:** English | [Français](../../docs/fr/skills/planner/SKILL.md)

Activated automatically when `task.assess` classifies the goal as **complex**
(also selectable manually).

1. Call `plan.update` first with atomic nodes — required before any side effect.
2. Mark independence: if two nodes do not need each other's output, they are **parallel**.
3. For each **heavy or parallel** node: `agent.spawn` with a **short self-contained brief** (≤3 sentences; limited skills/tools/docs — never dump parent tool results). Prefer parallel spawns over serial solo work when nodes are independent.
4. For each node / spawned brief: `memory.recall` on that narrow query (not the whole goal).
5. `agent.await` (or await several children) then integrate results; keep sequential only when a node truly depends on another.
6. Never pass more caps/tools/docs to a sub-agent than necessary; briefs stay lean so the parent context does not bloat.
7. Finish with `goal.complete` when all nodes are Done.
