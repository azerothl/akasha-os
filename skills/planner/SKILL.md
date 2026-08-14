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

1. Call `plan.update` with atomic nodes.
2. For each heavy subtask: `agent.spawn` with a narrow brief (limited skills/tools/docs).
3. `agent.await` or continue in parallel then integrate results.
4. Never pass more caps/tools to a sub-agent than necessary.
5. Finish with `goal.complete` when all nodes are Done.
