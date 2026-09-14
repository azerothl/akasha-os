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
2. **Conseil / évaluation** (« si je veux… », limitations, demandes d'évolution) : plan d'*analyse et réponse*, puis `goal.complete`. **Pas** de `module.scaffold` / install sauf demande explicite de construire maintenant.
3. Déléguer les nœuds lourds via `plan.delegate_step` (brief court auto-suffisant).
4. Quand un sous-agent délégué termine, le runtime injecte `[child-done]` et passe l'étape en Done. Si `agent.await` dit « toujours en cours », **réessayer** — l'enfant n'est pas bloqué ; ne pas refaire ses notes/scaffold.
5. Logs internes via `plan.append_log` — ne pas les dumper à l'utilisateur.
6. Terminer avec `goal.complete` quand les étapes critiques sont Done (ou quand la réponse d'évaluation est prête).
