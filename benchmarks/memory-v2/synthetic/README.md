# Corpus synthétique Memory V2

Ce corpus est destiné aux validations de rappel, déduplication, temporalité,
contradictions, permissions et navigation du graphe Memory V2. Il ne contient
aucune donnée personnelle réelle : les projets et personnes sont fictifs.

## Contenu

Le générateur produit, avec une graine fixe, 12 namespaces de projet et :

- 972 objets Memory V2 : entités, objectifs, problèmes, décisions, événements,
  affirmations et préférences ;
- des objets dans tous les états du cycle de vie ;
- des relations `targets`, `involves`, `depends_on`, `supports`,
  `derived_from`, `supersedes` et `contradicts` ;
- 192 requêtes annotées, dont des cas de décision, temporalité, contradiction,
  idempotence et isolation de namespace.

## Reproduire le corpus

Depuis la racine du dépôt :

```powershell
py -3 benchmarks/memory-v2/synthetic/generate_synthetic.py
```

Les fichiers générés sont :

- `create_requests.jsonl` : demandes compatibles avec `mem.object.create` ;
- `relations.jsonl` : relations exprimées avec les identifiants stables
  `from_fixture_id` et `to_fixture_id`, à remapper vers les IDs runtime ;
- `queries.jsonl` : requêtes et résultats attendus, avec une cible top-5 ;
- `manifest.json` : graine, volumes et seuils de validation.

Chaque objet porte un `metadata.fixture_id` et une clé d'idempotence. Un runner
doit conserver la correspondance `fixture_id → id` retournée par l'API, rejouer
les créations, puis vérifier que le nombre d'objets ne croît pas.

Le corpus est volontairement déterministe afin de comparer les résultats V1/V2
et les exécutions avec différents modèles d'embeddings. Il sert de validation
de pipeline ; la décision d'activer automatiquement Memory V2 doit encore être
prise sur un corpus anonymisé construit à partir de cas réels.
