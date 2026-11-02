# Ticket #460 — couche de consolidation sémantique

Traitement local du 22 octobre 2026, à partir de `main` (`1274b9f`).
Implémentation : `c682f9703f061f0a11385bd17b626cdbfa259639`.

## Analyse

La demande porte sur l'apprentissage de concepts à partir de plusieurs souvenirs,
avec profils utilisateur et agent, confiance, contradictions et sources
consultables. Le dépôt possède déjà Memory V2 : objets cognitifs typés, graphe,
preuves, journal de décisions et narrations périodiques. La partie manquante est
la consolidation des observations en interprétations durables.

La solution réutilise les identifiants et preuves V2. Une couche native du
service mémoire convient mieux qu'un module disposant de droits génériques sur
ses fichiers : les clients conservent des permissions mémoire par sujet, et les
concepts ne deviennent pas eux-mêmes des faits servant à les renforcer.

## Résultat implémenté

- Consolidation par modèle local, manuelle ou activée explicitement en
  arrière-plan, par lots reprenables et bornés.
- Concepts, préférences, convictions, tendances, objectifs, style de
  communication, méthodes réussies, échecs récurrents et outils préférés.
- Profils distincts `user:<id>` et `agent:<id>`, avec sources favorables et
  contradictoires, score de preuves calculé par l'hôte, dates et révisions.
- Graphe consultable concept → souvenir → source ou conversation d'origine.
- Confirmation humaine requise avant usage dans le contexte ; rejet et conflits
  restent visibles. Les agents ne peuvent ni activer le traitement périodique
  ni confirmer leurs propres propositions.
- État sémantique séparé des faits, commit atomique des lots, reprise après
  redémarrage, invalidation des sources modifiées et effacement des dérivés.
- Même autorisation pour la consultation directe et l'injection dans le contexte.
  Le worker transmet ses permissions réelles et dispose des outils natifs de
  consultation/consolidation.
- Panneau Mémoire FR/EN avec choix du profil, activation, revue, preuves et
  navigation vers les souvenirs et conversations disponibles.

Les limites et API exactes sont dans le
[contrat de la couche sémantique](../semantic-memory-layer.md).

## Vérifications observées

| Vérification | Résultat |
|---|---|
| Plateforme, bibliothèque sans fonctionnalités par défaut, exécution série | 335 tests réussis après la fusion |
| Agent, sans fonctionnalités par défaut | 384 tests de bibliothèque, 19 worker, 4 daemon réussis |
| Protocole | 194 tests réussis |
| Clippy strict, toutes cibles des quatre crates modifiées | Réussi, interface comprise |
| Nouveaux tests du ticket et de la reprise WASM | 16 plateforme et 1 worker réussis |
| Clippy strict, toutes les cibles de `aos-platform` après la fusion | Réussi |
| Vrai Qwen3.5-4B Q4_K_M, `aos-modeld`, broker et hôte isolés | Deux lots successifs, 16 observations synthétiques |

Soit 935 tests exécutés dans ces trois packages, sans compter les compilations
Clippy comme des tests supplémentaires. Les tests vérifient notamment les
permissions transmises par le worker, le refus de profils hors périmètre,
le refus sans résolution de la cap noyau, la non-injection des candidats,
la reprise, l'annulation d'une inférence en erreur et l'effacement de preuves
devenues historiques.

Avec Qwen, les observations utilisateur ont donné un concept de préférence pour
la performance native. Le profil agent a comporté une interprétation signalée
comme contradictoire ; sa confirmation a été refusée et son contexte est resté
vide. Ce classement par le modèle n'est pas une preuve que les observations
sont réellement contradictoires. Il montre que le conflit rapporté est exposé
et qu'il bloque l'usage automatique. Les confirmations du probe sont automatisées.

Le premier probe exigeait à tort un concept agent confirmable : il a donc échoué
sur cette interprétation contradictoire. Le probe final vérifie aussi le refus
attendu ; les deux rapports sont conservés dans la preuve.

Après fusion de la PR #461, le probe a été étendu à deux phases chronologiques.
Sur la séquence utilisateur, le modèle a d'abord résumé une préférence pour
l'inférence locale, puis l'a révisée vers un service cloud pour réduire la
maintenance. Cette révision est restée candidate et contestée, avec une ancienne
révision conservée ; le profil n'a pas été réinjecté dans le contexte. Sur la
séquence agent, le modèle a révisé son concept en gardant visibles les réussites
et échecs antérieurs ; ce concept est également resté contesté et hors contexte.
Les 16 sources ont été prises en compte, l'état a survécu à la réouverture du
store et le daemon détenu par le probe a été arrêté.

Cette mesure est une vérification exploratoire sur des phrases synthétiques
construites pour couvrir un changement de préférence. Elle vérifie le parcours
technique de révision et le retrait du contexte, mais ne mesure pas la justesse
sur des conversations représentatives. Rapport complet et empreintes dans
[2026-10-23-semantic-memory-longitudinal.json](evidence/2026-10-23-semantic-memory-longitudinal.json).

Le probe a révélé qu'une réponse modèle pouvait omettre des sources en attente
ou ajouter un champ d'historique non fiable. Le traitement exige maintenant un
accusé de prise en compte de chaque source en attente avant de valider un lot ;
si un accusé manque, les sources restent à traiter. Le parseur ignore uniquement
le champ `revision_history` spéculatif et reconstruit l'historique depuis les
preuves sauvegardées. Un test garantit qu'un lot incomplet reste reprenable.

Un passage parallèle en développement a rencontré le timeout de 30 secondes
déjà suivi dans [#456](https://github.com/azerothl/akasha-os/issues/456)
(332 réussis, 1 échec). Le test a ensuite réussi seul, puis dans les 334 tests
en série. Le premier passage CI de la PR #461 a reproduit cet échec
(341 réussis, 1 échec sous Linux). Le correctif de CI conserve le test et
surveille désormais le worker WASM en même temps que les confirmations, avec
un délai global de 180 secondes pour le scénario. Le worker conserve la fixture
jusqu'à sa sortie : un échec asynchrone ne supprime plus ses fichiers en cours
d'utilisation. Les erreurs du worker remontent immédiatement. Après correction,
les 334 tests plateforme locaux passent en parallèle et Clippy strict passe
pour toutes les cibles de la plateforme.

Le 2 novembre, le commit local `9c48704` a été conservé et la branche a intégré
`main` (`2470aa8`) puis la correction ZIP 9 de #468 et ses tests complémentaires.
Les 17 tests correspondant au filtre `semantic` passent avec les dépendances
actuelles (dont Wasmtime 48), ainsi que Clippy strict de la plateforme, toutes
cibles sans fonctionnalités par défaut. Les cinq tests ciblés de mise à jour
passent aussi. Le probe Qwen longitudinal cité ci-dessus reste celui du
23 octobre ; il n'a pas été rejoué le 2 novembre. La validation native et celle
sur historiques représentatifs restent ouvertes.

Preuves, empreintes des journaux et rapports initiaux/finals :
[2026-10-22-semantic-memory.json](evidence/2026-10-22-semantic-memory.json).

## Acceptation restante

La PR #461 est fusionnée ; aucune nouvelle release n'a été publiée. Le parcours
dans l'interface native installée n'a pas été observé et reste explicitement
ouvert. La qualité sur des historiques représentatifs et évolutifs reste à
évaluer ; seize phrases synthétiques ne prouvent pas un gain de personnalisation,
de raisonnement ou de performance.

Le vieillissement automatique, la fusion entre concepts et les profils partagés
entre agents sont les extensions futures du ticket. La consolidation actuelle
respecte ses bornes documentées ; elle ne prétend pas constituer un graphe
hiérarchique sans limite.

Le ticket #460 reste ouvert pour la validation native et une évaluation de qualité
sur des historiques représentatifs. Les tickets #247, #434 et #435 conservent leur
statut ouvert.
