# AK-004 — Parcours UI natifs non automatisables dans la recette

- Priorité : P1
- Statut : corrigé pour le périmètre CI local — smoke-test headless ajouté
- Zone : harness de test / CI Windows

Le bus permet de vérifier les services, mais la session de test ne dispose pas d’un pilotage natif de la fenêtre egui. Les boutons, focus clavier, lecture audio/vidéo, tailles de fenêtre et états d’erreur restent donc non vérifiés automatiquement.

Un mode `AOS_UI_SELF_TEST=1` est désormais disponible dans `aos-ui-egui` ; il a passé la recette avec `min_inner=702x600 locale=fr/en theme=custom`. Il vérifie sans ouvrir de fenêtre les dimensions minimales, les chaînes FR/EN et la résolution des tokens de thème. Les tests de clic/focus, captures DPI ou lecture multimédia restent à brancher sur un runner Windows avec capture native ; cette dépendance externe est documentée plutôt que masquée.
