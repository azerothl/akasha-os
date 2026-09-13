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
  idempotence et isolation de namespace ; chaque cas indique l'endpoint et
  l'assertion à évaluer.

## Reproduire le corpus

Depuis la racine du dépôt :

```powershell
python benchmarks/memory-v2/synthetic/generate_synthetic.py
```

Préparer les lots destinés au client batch Preview :

```powershell
python benchmarks/memory-v2/synthetic/prepare_batches.py --output .tmp/memory-v2-batch
```

Après l'ingestion V2, préparer les lots dépendant des IDs runtime :

```powershell
python benchmarks/memory-v2/synthetic/prepare_batches.py `
  --output .tmp/memory-v2-batch `
  --runtime-results .tmp/memory-v2-run/v2-create-results.json
```

Les fichiers générés sont :

- `create_requests.jsonl` : demandes compatibles avec `mem.object.create` ;
- `relations.jsonl` : relations exprimées avec les identifiants stables
  `from_fixture_id` et `to_fixture_id`, à remapper vers les IDs runtime ;
- `queries.jsonl` : requêtes, endpoint cible et assertions attendues ; seules
  les requêtes `mem.context` utilisent une cible top-5 ;
- `manifest.json` : graine, volumes et seuils de validation.

Chaque objet porte un `metadata.fixture_id` et une clé d'idempotence. Un runner
doit conserver la correspondance `fixture_id → id` retournée par l'API, rejouer
les créations, puis vérifier que le nombre d'objets ne croît pas.

Les assertions ne sont pas toutes des recherches sémantiques : le manifest
répartit les requêtes entre `mem.context`, `mem.graph.query`, `mem.timeline`,
`mem.object.list`, `mem.explain` et `mem.narrative.generate`. Le rapport doit
donc être lu par endpoint et par scénario, avec le top-5 réservé aux cas
explicitement sémantiques.

Le corpus est volontairement déterministe afin de comparer les résultats V1/V2
et les exécutions avec différents modèles d'embeddings. Il sert de validation
de pipeline ; la décision d'activer automatiquement Memory V2 doit encore être
prise sur un corpus anonymisé construit à partir de cas réels.

## Exécution complète sur Preview

Après avoir démarré `aos-platformd` avec `AOS_MEMORY_V2=1` et
`AOS_MEMORY_V2_SHADOW=1`, une exécution complète (ingestion V2, relations,
rejeu idempotent, écritures V1, appels API, puis rapport) se lance ainsi :

```powershell
powershell -ExecutionPolicy Bypass -File benchmarks/memory-v2/synthetic/run_preview_benchmark.ps1
```

Le script utilise le bus `127.0.0.1:24701` et écrit ses résultats dans
`.tmp/memory-v2-run`. Les options `-Bus`, `-Output`, `-Probe`, `-SkipV1` et
`-SkipLlm` permettent d'adapter l'exécution. La partie LLM récupère d'abord un
contexte réel via `mem.context`, puis soumet ce contexte à `model.infer` et
vérifie les termes attendus ainsi qu'une citation `[memory:ID]`.
