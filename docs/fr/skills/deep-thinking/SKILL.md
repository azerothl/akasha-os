---
name: deep-thinking
description: Plans Deep Thinking hiérarchiques avec révision dynamique et délégation
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

Activé quand `cognitive_mode` vaut `deep_thinking` (flag de requête ou phrase utilisateur).

1. Appeler d'abord `plan.create` avec un **arbre hiérarchique complet**.
2. Déléguer les nœuds lourds via `plan.delegate_step` (brief court auto-suffisant).
3. Quand un sous-agent délégué termine, le runtime injecte `[child-done]` et passe l'étape en Done. Ensuite `plan.update_step` ou `plan.replace_tree` si le plan doit changer. Ne pas attendre indéfiniment.
4. Logs internes via `plan.append_log` — ne pas les dumper à l'utilisateur.
5. Terminer avec `goal.complete` quand les étapes critiques sont Done.
