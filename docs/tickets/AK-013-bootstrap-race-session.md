# AK-013 — Bootstrap UI sensible au démarrage du bus

- Priorité : P1
- Statut : corrigé, déployé et vérifié sur démarrage devancé
- Zone : runtime UI egui, démarrage et chargement des sessions

## Constat

Quand `aos-ui-egui` démarrait avant `aos-busd`/`aos-platformd`, le runtime quittait définitivement sa boucle sur une erreur de connexion. Même lorsque le bus devenait ensuite disponible, l’interface restait vide avec « Un problème est survenu. Réessayez. » et ne rechargeait pas les sessions.

## Correction

La connexion au bus est maintenant réessayée toutes les 500 ms tant que le superviseur n’est pas prêt. Le premier `chat.session.list` et la création de session par défaut disposent également de huit tentatives espacées, avec une erreur détaillée seulement après épuisement.

## Vérification

Le parcours a été reproduit en lançant l’UI avant `aos-session`, puis en démarrant le superviseur : l’UI a attendu la disponibilité du bus et a chargé les sessions persistées, sans rester sur l’erreur générique. La suite UI (375 tests) et `cargo check --workspace` passent.
