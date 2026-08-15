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
2. For each node / spawned brief: `memory.recall` on that narrow query (not the whole goal).
3. For each heavy subtask: `agent.spawn` with a narrow brief (limited skills/tools/docs).
4. `agent.await` or continue in parallel then integrate results.
5. Never pass more caps/tools to a sub-agent than necessary.
6. Finish with `goal.complete` when all nodes are Done.
