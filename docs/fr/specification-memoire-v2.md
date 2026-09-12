# Spécification de mise en œuvre de la mémoire V2

**Statut :** proposition de mise en œuvre  
**Cible :** Akasha OS Preview puis v1  
**Dépendances :** `aos-platform`, `aos-proto`, `aos-agent`, interface egui

## 1. Résumé de la décision

Memory V2 transforme la mémoire actuelle d'un stockage de faits et de
documents en une mémoire cognitive structurée. La mémoire doit pouvoir
répondre à « pourquoi avons-nous pris cette décision ? », « comment ce projet
a-t-il évolué ? » et « sur quelles preuves repose cette affirmation ? ».

La mise en œuvre sera progressive et compatible avec les APIs `mem.*` déjà
présentes. Elle ne nécessite pas de réécrire le kernel, le bus CBOR ou le
runtime des agents.

La priorité de la première version est la **mémoire des décisions**, suivie du
graphe temporel, de l'explicabilité et des timelines de projets. Les fonctions
de type Mind Palace et les narrations autobiographiques viendront ensuite.

## 2. Objectifs

Memory V2 doit permettre de :

1. représenter des concepts plutôt que seulement des extraits de documents ;
2. relier projets, personnes, objectifs, problèmes, décisions et sources ;
3. conserver l'évolution d'une information dans le temps ;
4. modéliser les décisions avec leurs motifs, alternatives, participants,
   risques et conséquences ;
5. pondérer chaque information par sa confiance, sa fraîcheur et son
   importance ;
6. expliquer chaque réponse par des preuves consultables ;
7. laisser l'utilisateur confirmer, corriger, masquer, exporter et supprimer
   sa mémoire ;
8. générer des timelines et des synthèses uniquement à partir d'objets
   sourcés.

## 3. Hors périmètre initial

Ne font pas partie du premier incrément :

- connecteurs directs Slack, Gmail ou calendrier ;
- support multi-utilisateur et multi-tenant ;
- marketplace publique de modèles de mémoire ;
- suppression automatique d'informations devenues anciennes ;
- enregistrement ou restitution du raisonnement interne du modèle ;
- remplacement du `MemoryStore` par une base externe obligatoire ;
- modification du kernel ou du protocole CBOR existant.

Les connecteurs pourront alimenter ultérieurement le même pipeline d'ingestion
que les documents, notes et conversations.

## 4. État actuel et stratégie de compatibilité

L'implémentation actuelle fournit déjà :

- mémoire de travail par agent ;
- entrées épisodiques persistées en JSONL ;
- embeddings et recherche vectorielle ;
- relations `similar`, `updates` et `supersedes` ;
- extraction automatique de faits depuis le chat ;
- bibliothèque utilisateur de documents PDF, TXT et Markdown ;
- rappel de mémoire dans le contexte des agents.

Les limites principales sont les suivantes :

- `EpisodicEntry` reste centré sur `text + metadata + vector` ;
- les types cognitifs ne sont pas explicitement représentés ;
- la temporalité est surtout un timestamp et une supersession ;
- la mémoire partagée est actuellement en mémoire de processus et doit devenir
  persistante ;
- l'index vectoriel est en recherche brute, adaptée à une petite collection
  mais pas à une mémoire de plusieurs années ;
- l'interface expose surtout une liste et un rappel, pas un graphe, une
  timeline ou une explication structurée.

La compatibilité est obligatoire : les anciens appels `mem.episodic_*`,
`mem.user.*` et `mem.context` continuent de fonctionner pendant la migration.

## 5. Modèle conceptuel

### 5.1 Types d'objets

Les objets suivants constituent l'ontologie minimale :

| Type | Rôle |
|---|---|
| `document` | Source brute ou document importé |
| `event` | Fait daté qui s'est produit ou a été observé |
| `entity` | Projet, personne, équipe, outil ou organisation |
| `claim` | Affirmation susceptible d'être confirmée ou contredite |
| `decision` | Choix effectué avec contexte et alternatives |
| `goal` | Résultat recherché |
| `problem` | Blocage, risque ou question ouverte |
| `preference` | Préférence attribuée à une personne ou un agent |
| `narrative` | Synthèse générée à partir d'objets sourcés |

Une relation est une arête typée entre deux objets ; elle ne doit pas être
stockée uniquement comme du texte dans un prompt.

### 5.2 Champs communs

Chaque objet doit contenir :

```text
id                 identifiant stable
kind               type d'objet
title              libellé court
content            contenu structuré ou résumé
namespace          espace de sécurité et de collaboration
created_at         date de création de l'objet
observed_at        date à laquelle l'information a été observée
valid_from         début de validité connu
valid_until        fin de validité connue, optionnelle
confidence         confiance entre 0.0 et 1.0
importance         importance entre 0.0 et 1.0
freshness          fraîcheur calculée entre 0.0 et 1.0
last_used_at       dernier usage dans une réponse ou un rappel
source_refs        preuves d'origine
visibility         public, private ou secret
status             active, disputed, superseded ou archived
metadata           extension compatible et non critique
```

`confidence` mesure la solidité des preuves, pas la vérité absolue. Une
information ancienne peut rester vraie ; elle devient seulement moins sûre ou
moins pertinente tant qu'elle n'est pas reconfirmée.

### 5.3 Relations minimales

Les relations existantes sont conservées et complétées par :

```text
contains          document -> event / claim
mentions          source -> entity
about             object -> entity
involves          decision / event -> entity
supports         source / claim -> object
contradicts      object -> object
depends_on       object -> object
part_of          object -> project / narrative
derived_from     object -> source / object
causes           event / decision -> consequence
targets          decision / task -> goal
```

Les relations doivent porter leur propre date, confiance, source et visibilité
lorsqu'elles peuvent révéler une information sensible.

### 5.4 Objet décision

La décision est le premier objet riche à implémenter :

```text
Decision {
  id
  question
  chosen_option
  alternatives[]
  rationale[]
  participants[]
  constraints[]
  risks[]
  consequences[]
  confidence
  decided_at
  valid_until?
  status
  evidence[]
}
```

Exemple logique :

```json
{
  "kind": "decision",
  "title": "Choisir Rust pour Akasha",
  "chosen_option": "Rust",
  "alternatives": ["C++", "Zig"],
  "rationale": [
    "sécurité mémoire",
    "écosystème WASM",
    "compatibilité avec seL4"
  ],
  "participants": ["Loïc", "Agent Architecture"],
  "risks": ["courbe d'apprentissage"],
  "decided_at": "2026-02-12",
  "confidence": 0.82,
  "evidence": ["source:chat:session-123", "source:doc:architecture.md"]
}
```

## 6. Architecture cible

```text
Sources brutes
  documents / notes / conversations / événements
          |
          v
Ingestion et normalisation
  texte, auteur, dates, classification, empreintes
          |
          v
Extraction cognitive
  événements, entités, claims, décisions, relations
          |
          v
Store canonique
  objets versionnés + journal + preuves
          |
          +--> Graphe temporel
          +--> Index lexical / vectoriel
          +--> Index de preuves
          |
          v
API de mémoire
  recall / timeline / graph / explain / narrative
          |
          v
Agents et interface humaine
```

Le chemin canonique reste le bus d'intentions et les types `aos-proto`. Le
stockage interne peut rester dans `aos-platform` dans un premier temps.

### 6.1 Composants à ajouter ou faire évoluer

- `aos-proto` : types d'objets, relations, preuves et requêtes V2 ;
- `aos-platform::memory` : stockage versionné, projection du graphe et
  vieillissement ;
- `aos-platform::extract` : extraction typée, pas seulement une liste de faits ;
- `aos-platform::user_docs` : émission de références de sources et d'événements
  d'ingestion ;
- `aos-platform::boot_index` : indexation asynchrone et reprise ;
- `aos-agent` : bootstrap enrichi et citations dans les réponses ;
- egui : décisions, timeline, graphe, preuves et confirmation de souvenirs.

## 7. Pipeline d'ingestion

Pour chaque source :

1. calculer une empreinte et identifier la source ;
2. extraire le texte, l'auteur et les dates disponibles ;
3. appliquer la classification `public`, `private` ou `secret` ;
4. créer les événements et références brutes ;
5. extraire les objets cognitifs avec le modèle local ;
6. résoudre les entités connues sans inventer de personnes ou de projets ;
7. calculer les relations et les preuves ;
8. dédupliquer ou versionner les objets existants ;
9. écrire le journal et mettre à jour les index ;
10. rendre le résultat visible dans l'audit et l'interface.

L'extraction doit être idempotente : réindexer le même document ne doit pas
produire une nouvelle décision ou un nouvel événement à chaque démarrage.

Une erreur d'extraction ne doit jamais supprimer la mémoire existante. Une
nouvelle information contradictoire crée une nouvelle version ou un objet
`disputed` jusqu'à résolution.

## 8. Temporalité et vieillissement

Le système distingue :

- la date de création de l'objet ;
- la date d'observation de la preuve ;
- la période pendant laquelle l'objet est valide ;
- la date de dernière confirmation ;
- la date de dernière utilisation.

Le score de rappel proposé est :

```text
score = pertinence sémantique
      * poids_confiance
      * poids_fraîcheur
      * poids_visibilité
      + bonus_importance
      + bonus_pin
```

Le vieillissement est une fonction de classement, pas une suppression. Pour une
information sensible ou fortement obsolète, Akasha peut demander :

> « Cette information n'a pas été confirmée depuis longtemps. Veux-tu la
> mettre à jour ? »

Les objets supersédés restent exportables et consultables avec une option
explicite, afin de préserver l'histoire du projet.

## 9. Explicabilité

Toute réponse générée à partir de Memory V2 doit pouvoir retourner un bloc de
preuves :

```text
claim: Rust est la préférence actuelle
confidence: 0.91
evidence:
  - 12 conversations
  - 5 projets
  - 3 décisions
last_confirmed: 2026-09
```

L'API doit distinguer :

- les preuves effectivement retrouvées ;
- les inférences calculées à partir de ces preuves ;
- les éléments incertains ou contradictoires.

Les preuves auxquelles l'appelant n'a pas accès ne doivent pas apparaître sous
forme de résumé indirect. Les caps et la classification s'appliquent à la
réponse d'explication comme à la recherche initiale.

## 10. APIs proposées

Les APIs existantes restent valides. Les nouvelles intentions peuvent être
introduites par étapes :

```text
mem.object.create        créer un objet cognitif
mem.object.get           lire un objet et ses relations autorisées
mem.object.update        créer une nouvelle version ou corriger
mem.object.list          filtrer par type, namespace et période
mem.graph.query          obtenir un sous-graphe borné
mem.timeline             reconstruire l'histoire d'un projet ou sujet
mem.decision.get         récupérer une décision et ses preuves
mem.explain              expliquer une affirmation ou un hit mémoire
mem.revalidate           demander une confirmation utilisateur
mem.narrative.generate   générer une synthèse sourcée
```

Chaque réponse doit inclure des limites explicites : profondeur maximale du
graphe, nombre de nœuds, budget de texte et sources masquées par permission.

Les réponses `mem.context` actuelles pourront progressivement inclure :

```text
objects[]
relations[]
evidence[]
temporal_warnings[]
prompt_block
```

## 11. Sécurité et gouvernance

- Aucun agent ne reçoit un accès implicite à toute la mémoire.
- Les namespaces, caps et classifications filtrent objets, relations et
  preuves.
- `secret` ne doit jamais être envoyé à un modèle distant.
- Une extraction automatique ne peut pas abaisser une classification.
- Toute correction, suppression, reclassification et revalidation est auditée.
- Les narrations sont régénérables et supprimables ; elles ne deviennent pas
  automatiquement une vérité canonique.
- Une décision générée sans preuve suffisante doit être marquée incertaine ou
  soumise à confirmation.
- La mémoire partagée entre agents doit être persistée avec propriétaire,
  participants autorisés, version et conflit éventuel.

## 12. Interface utilisateur

### Incrément 1

Ajouter à l'onglet Memory :

- filtre par type d'objet ;
- fiche de décision ;
- sources et niveau de confiance ;
- statut actif, contesté ou supersédé ;
- confirmation, correction, suppression et export.

### Incrément 2

Ajouter :

- timeline d'un projet ;
- affichage des changements d'opinion ;
- avertissements d'obsolescence ;
- vue « pourquoi cette réponse ? ».

### Incrément 3

Ajouter :

- navigation graphe / Mind Palace ;
- résumés hebdomadaires et mensuels ;
- narration annuelle et autobiographique, toujours avec preuves.

Les vues graphiques doivent rester bornées et lisibles. Une réponse textuelle
avec sources reste le mode de repli lorsque le graphe est trop large.

## 13. Migration des données

La migration doit être sans rupture :

1. conserver les IDs des entrées `EpisodicEntry` ;
2. mapper `fact` vers `claim` et `episode` vers `event` par défaut ;
3. conserver `namespace`, `metadata`, `vector`, `ts_ms` et `pinned` ;
4. mapper `similar`, `updates` et `supersedes` vers les relations V2 ;
5. ajouter les champs absents avec des valeurs explicites ;
6. reconstruire les index dans un job asynchrone ;
7. ne supprimer les anciens journaux qu'après vérification et sauvegarde ;
8. permettre un retour au chemin de lecture V1 en cas d'échec.

Le format de stockage recommandé est un journal append-only d'événements de
mémoire et des projections dérivées pour le graphe et les index. Les
projections peuvent être reconstruites sans perdre la source canonique.

## 14. Découpage de réalisation

### Memory V2.1 — Décisions sourcées

- types `MemoryObject`, `Decision`, `Evidence` et relations de base ;
- extraction de décisions depuis chat et documents ;
- stockage versionné et migration V1 ;
- API `mem.decision.get` et `mem.explain` ;
- fiche de décision dans l'interface.

### Memory V2.2 — Graphe temporel

- périodes de validité et contradictions ;
- vieillissement et demande de revalidation ;
- API `mem.timeline` et `mem.graph.query` ;
- mémoire partagée persistante ;
- indexation asynchrone robuste.

### Memory V2.3 — Narration et navigation

- résumés périodiques sourcés ;
- navigation Mind Palace bornée ;
- synthèse de projet et histoire utilisateur ;
- index ANN lorsque les volumes le justifient ;
- benchmark de qualité et de coût.

## 15. Critères d'acceptation

La version V2.1 est acceptable si :

- une décision extraite peut être retrouvée par sa question, son choix ou son
  projet ;
- une décision affiche ses alternatives, motifs, participants et preuves ;
- une nouvelle décision contradictoire ne détruit pas l'ancienne ;
- une information ancienne peut être signalée comme potentiellement obsolète ;
- toute affirmation Memory V2 peut retourner ses preuves autorisées ;
- un utilisateur peut corriger et supprimer un objet ;
- une réindexation est idempotente ;
- un agent ne voit pas les objets hors de ses caps ;
- les secrets ne sont ni extraits automatiquement ni transmis à un backend
  distant ;
- les APIs V1 continuent de fonctionner ;
- les écritures et décisions importantes apparaissent dans l'audit.

Les objectifs de benchmark à valider sur un jeu de test construit à partir de
cas réels sont :

- rappel de la bonne décision dans le top 5 : au moins 90 % ;
- réponses narratives avec preuve manquante : 0 dans le jeu de test ;
- doublons créés par une réindexation identique : 0 ;
- accès non autorisé à une preuve : 0.

## 16. Questions à trancher avant le code

1. Faut-il conserver JSONL comme format canonique ou migrer vers SQLite après
   validation de l'ontologie ?
2. Quelles familles d'objets peuvent être extraites automatiquement sans
   confirmation ?
3. Quel modèle local produit les embeddings et l'extraction structurée avec
   le meilleur compromis CPU/GPU ?
4. Quelle durée de conservation appliquer aux événements et aux narrations ?
5. La mémoire partagée doit-elle être par projet, par équipe ou par agent ?
6. Quels seuils déclenchent une demande de revalidation ?

Ces décisions ne doivent pas bloquer le premier vertical : la mémoire des
décisions peut commencer avec des documents, des notes et des conversations,
un namespace utilisateur et une extraction locale contrôlée.
