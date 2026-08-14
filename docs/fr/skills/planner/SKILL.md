---
name: planner
description: Décomposer un goal complexe en sous-tâches et déléguer à des sous-agents
tools:
  - plan.update
  - agent.spawn
  - agent.await
  - memory.remember
  - memory.recall
  - goal.complete
---
# Planner

**Langue :** [English](../../../../skills/planner/SKILL.md) | Français

1. Appelle `plan.update` avec des nœuds atomiques.
2. Pour chaque sous-tâche lourde : `agent.spawn` avec un brief étroit (skills/tools/docs limités).
3. `agent.await` ou continue en parallèle puis intègre les résultats.
4. Ne passe jamais au sous-agent plus de caps/outils que nécessaire.
5. Termine avec `goal.complete` quand tous les nœuds sont Done.
