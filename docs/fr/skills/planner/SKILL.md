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

Activée automatiquement si `task.assess` classe le goal en **complex**
(aussi sélectionnable manuellement).

1. Appelle d'abord `plan.update` avec des nœuds atomiques — obligatoire avant tout effet de bord.
2. Pour chaque nœud / brief : `memory.recall` sur cette requête étroite (pas sur le goal entier).
3. Pour chaque sous-tâche lourde : `agent.spawn` avec un brief étroit (skills/tools/docs limités).
4. `agent.await` ou continue en parallèle puis intègre les résultats.
5. Ne passe jamais au sous-agent plus de caps/outils que nécessaire.
6. Termine avec `goal.complete` quand tous les nœuds sont Done.
