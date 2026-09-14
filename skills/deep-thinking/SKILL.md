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
2. **Advisory / evaluation** goals (« if I wanted to… », limitations, evolution requests): plan to *analyze and answer*, then `goal.complete`. Do **not** `module.scaffold` / install unless the user explicitly asks to build now.
3. Mark heavy or parallel nodes; use `plan.delegate_step` with a **short self-contained brief** (≤3 sentences).
4. When a delegated child finishes, the runtime injects `[child-done]` and marks the step Done. If `agent.await` says still running, **retry await** — the child is not blocked; do not redo its notes/scaffold.
5. Keep internal detail in `plan.append_log` — never dump logs into the user-facing answer.
6. `plan.get` when you need to re-read the tree; the UI shows a collapsible plan + light traces.
7. Finish with `goal.complete` when all critical steps are Done (or the evaluation answer is ready).
