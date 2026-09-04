---
name: deep-thinking
description: Hierarchical Deep Thinking plans with dynamic revision and sub-agent delegation
license: MIT
tools:
  - plan.create
  - plan.update_step
  - plan.replace_tree
  - plan.delegate_step
  - plan.get
  - plan.append_log
  - agent.spawn
  - agent.await
  - memory.remember
  - memory.recall
  - goal.complete
---
# Deep Thinking

**Language:** English | [Français](../../docs/fr/skills/deep-thinking/SKILL.md)

Activated when `cognitive_mode` is `deep_thinking` (request flag or user phrase).

1. Call `plan.create` first with a **full hierarchical tree** (`steps` + optional `children`) — required before any side effect.
2. Mark heavy or parallel nodes; use `plan.delegate_step` with a **short self-contained brief** (≤3 sentences).
3. After each result: `plan.update_step` (status) or `plan.replace_tree` if the plan must change.
4. Keep internal detail in `plan.append_log` — never dump logs into the user-facing answer.
5. `plan.get` when you need to re-read the tree; the UI shows a collapsible plan + light traces.
6. Finish with `goal.complete` when all critical steps are Done.
