---
name: planner
description: Décomposer un goal complexe en sous-tâches et déléguer à des sous-agents
license: MIT
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
2. Indépendance : si deux nœuds n'ont pas besoin du résultat l'un de l'autre, ils sont **parallèles**.
3. Pour chaque nœud **lourd ou parallèle** : `agent.spawn` avec un **brief court auto-suffisant** (≤3 phrases ; skills/tools/docs limités — jamais de dump des résultats d'outils du parent). Préfère des spawns parallèles plutôt que tout faire en série toi-même quand les nœuds sont indépendants.
4. Pour chaque nœud / brief : `memory.recall` sur cette requête étroite (pas sur le goal entier).
5. `agent.await` (ou plusieurs enfants) puis intègre ; ne reste séquentiel que si un nœud dépend vraiment d'un autre.
6. Ne passe jamais au sous-agent plus de caps/outils/docs que nécessaire ; briefs lean pour ne pas gonfler le contexte parent.
7. Termine avec `goal.complete` quand tous les nœuds sont Done.
