# Essai — taxonomie de pose Illustration

**Statut :** essai / proposition  
**Branche :** `cursor/illustration-pose-taxonomy`  
**Base :** `cursor/illustration-surface` (`ef5ad3d`)  
**Langue :** Français | [English](../illustration-pose-taxonomy.md)

## 1. Résumé

Cet essai explore une **troisième voie** entre :

1. le pipeline **SD multi-passes** (`illust.generate_image`) — génération /
   édition d’image pilotée par texte, où l’agent ne dessine pas ;
2. le pipeline **vectoriel actuel** (`illust.compose` + recettes) — l’agent
   (ou des templates pauvres) pose des formes, avec un rendu souvent
   illisible hors chat / chien / éléphant / pingouin / personne.

L’objectif est qu’un **agent dessine en autonomie** sur une **ossature
fiable** : classification en cascade (famille → groupe → type), squelette
déterministe (éventuellement affiné par un petit modèle), puis habillage
par l’agent.

Ce document est la **trace vivante** de l’essai : décisions, hors-périmètre,
contrats, critères de succès et journal d’évolution.

## 2. Problème

| Approche | Force | Faiblesse pour « agent qui dessine » |
|---|---|---|
| SD par passes | Rendu riche | Ce n’est pas un dessin trait par trait ; l’agent dirige / critique seulement |
| Vectoriel libre (LLM) | L’agent compose | Ellipses / formes plates ; sujets hors recettes illisibles |
| Recettes fixes | Lisibilité sur 4–5 espèces | Pas de généralité |

On veut **contrôler la pose** sans enfermer tous les sujets dans une liste
plate, et sans basculer par défaut sur la diffusion.

## 3. Objectifs

1. Classer un brief en **famille → groupe → type** avec **défauts** à chaque
   niveau si le détail est inconnu ou peu confiant.
2. Matérialiser un **squelette** (`IllustrationSpec.skeleton` + contacts)
   déterministe à partir du nœud choisi.
3. Laisser l’agent **habiller** (volumes, contours, accessoires) en liant
   les parts aux joints (`joint_bindings`), sans réinventer l’anatomie.
4. Garder un **fallback explicite** si même la famille est incertaine
   (abstention → compose libre contraint, ou autre pipeline).
5. Pouvoir brancher plus tard :
   - un **Choice Jevlike** (`E:\rlcd_model`) pour choisir le nœud ;
   - une **petite régression / séquence** pour affiner les joints.

## 4. Hors périmètre (premier incrément)

- Remplacer ou supprimer le pipeline SD.
- Entraîner / intégrer Jevlike dans `aos-platformd` dès le premier commit.
- Un modèle de régression de joints en production.
- Une taxonomie biologique exhaustive.
- Animation / engines sand-paper-found liés à cette taxonomie.
- Validation artistique automatique (vision critic).

## 5. Architecture cible

```text
brief (subject)
    │
    ▼
Classifieur cascade (règles d’abord ; Jevlike plus tard)
    famille → groupe → type   (+ confiance / abstention)
    │
    ▼
Poseur déterministe
    nœud → skeleton + contacts + contraintes de famille
    │  (optionnel plus tard : régression affine les joints)
    ▼
Agent (compose / passes construction)
    habillage path lié aux joints — interdit de remplacer le squelette
    │
    ▼
review structurelle → render_sheet → inspection → export
```

### 5.1 Rôles

| Composant | Produit | Ne produit pas |
|---|---|---|
| Taxonomie + classifieur | id de nœud, confiance | pixels, paths décoratifs |
| Poseur | `skeleton`, contacts, éventuellement volumes de base | style crayon / aquarelle |
| Agent | contours, détails, props liés | nouvelles articulations hors plan |
| Review | erreurs structurelles | score artistique |

### 5.2 Lien avec le code existant

Socle déjà présent dans `crates/aos-proto/src/illustration.rs` :

- `IllustrationSkeletonJoint`, `joint_bindings`, contacts IK ;
- `human_pose_joints`, `ensure_construction_passes` ;
- `construction_plan_for_subject` (sémantique, pas encore taxonomique).

Socle Jevlike (`E:\rlcd_model`) : Choice / Score / Noul — adapté à des
**menus bornés conditionnels**, pas à de la régression de coords.

## 6. Taxonomie (brouillon v0)

Trois niveaux maximum. Libellés **pragmatiques pour dessiner** (pas une
encyclopédie). Les noms naturels servent à l’UX et aux datasets.

### 6.1 Niveau famille (exemples)

| Id | Défaut squelette |
|---|---|
| `humanoid` | bipède, tête, 2 bras, 2 jambes |
| `mammal_quadruped` | corps horizontal, 4 pattes, tête |
| `bird` | corps, 2 pattes, ailes / volumes d’aile |
| `reptile` | corps allongé, 4 membres ou absence selon groupe |
| `amphibian` | comme reptile / quadrupède bas |
| `fish_bony` | corps fuselé, nageoires |
| `fish_cartilaginous` | corps fuselé, ailerons |
| `invertebrate` | plan minimal (segments / symétrie) |
| `object` | boîte / masse + appuis |
| `nature` | masses végétales / sol, pas d’anatomie animale |
| `scene_prop` | support (banc, fauteuil, vélo) |
| `unknown` | abstention famille → fallback |

### 6.2 Exemples de descentes

```text
humanoid → primate → human
humanoid → primate → lemur
mammal_quadruped → carnivore → cat | dog | bear
mammal_quadruped → ungulate → …
invertebrate → arachnid → scorpion | spider
object → furniture → bench | armchair
nature → flora → tree | flower
```

À chaque niveau, si le Choice inférieur s’abstient : **garder le défaut du
niveau supérieur**.

### 6.3 Scènes composites

Un brief « homme à la pipe dans le jardin » produit **plusieurs nœuds**
(sujet + props + décor), pas une seule famille fourre-tout. Le sujet
principal porte le squelette animable ; le décor utilise `nature` /
`scene_prop`.

## 7. Politique de décision

1. **Connu + confiant** : descendre jusqu’au type → pose template (+
   régression plus tard) → agent habille.
2. **Famille connue, détail flou** : s’arrêter au groupe ou à la famille →
   défauts → agent habille.
3. **Abstention famille** (`unknown` / faible confiance) :
   - ne **pas** forcer un mauvais template ;
   - fallback documenté : compose libre avec review stricte, **ou**
     bascule explicite SD / autre moteur (hors premier incrément code).
4. Interdit : inventer un type hors catalogue en silence. Soit nœud
   catalogue, soit abstention.

## 8. Contrats (spécification minimale)

### 8.1 ClassificationResult (brouillon)

```text
family_id: string
group_id: string | null          # défaut famille si null
type_id: string | null           # défaut groupe/famille si null
confidence: 0..1                 # ou Score Jevlike
abstain: bool
secondary_nodes: [ClassificationResult]  # props / décor
```

### 8.2 PoseInstance (brouillon)

```text
node_id: string                  # type ou groupe ou famille
skeleton: [IllustrationSkeletonJoint]
contacts: […]
joint_bindings_hints: { part_role → [joint_a, joint_b] }
pose_tags: [sitting, holding_pipe, …]   # optionnel
```

### 8.3 Règle agent

Après un `PoseInstance` valide :

- `illust.compose` **conserve** `skeleton` / contacts ;
- l’enrichissement **ne remplace pas** un squelette taxonomique par une
  recette espèce opaque ;
- l’habillage ajoute des `path` liés, pas un nouveau pantin ellipse.

## 9. Plan d’incréments

| # | Livrable | Statut |
|---|----------|--------|
| T0 | Spec + journal (ce document) | fait |
| T1 | Catalogue YAML/JSON famille→groupe→type + défauts squelette (humain, chat, objet/support) | fait |
| T2 | Classifieur déterministe par mots-clés du brief → `ClassificationResult` | fait |
| T3 | Poseur : nœud → `skeleton` branché sur `enrich` / `ensure_construction_passes` | fait |
| T4 | Tests proto + 3 prompts (jardinier, chat, sujet OOD) | fait |
| T4.5 | Protéger squelette taxonomique ; poses secondaires ; review skeleton-aware ; merge compose | fait |
| T5 | (Optionnel) dataset Choice Jevlike + évaluation hors processus Preview | plus tard |
| T6 | (Optionnel) tête de régression de joints | plus tard |

## 10. Critères de succès (essai)

**Réussi si :**

- un brief « jardinier / pipe / jardin » produit un squelette humain assis
  lisible **sans** SD ;
- un brief chat produit un quadrupède carnivore sans retomber sur le blob
  générique hors recette opaque ;
- un brief volontairement OOD (sujet hors catalogue) **s’abstient** ou
  reste au défaut de famille, sans planter ni inventer un type fantôme ;
- l’agent peut ajouter des contours liés sans écraser le squelette ;
- la review structurelle refuse un habillage ellipse-only quand un squelette
  taxonomique est présent.

**Échec si :**

- la taxonomie bloque tout sujet non listé ;
- le rendu reste une pile d’ellipses indiscernables sur les cas T4 ;
- on réintroduit le SD comme fallback silencieux « pour que ça ait l’air
  joli ».

## 11. Risques

| Risque | Mitigation |
|---|---|
| Catalogue trop large trop tôt | 3 niveaux ; démarrer avec humanoid + mammal_quadruped + object + nature |
| Sur-confiance du classifieur | seuil + abstention explicite |
| Agent qui ignore le squelette | contrat outil + tests + enrich qui protège le skeleton |
| Confusion avec le pipeline image | doc + flag / chemin vectoriel séparé ; pas de mélange dans T1–T4 |

## 12. Journal d’évolution

| Date | Événement |
|---|---|
| 2026-09-20 | Branche `cursor/illustration-pose-taxonomy` créée depuis `illustration-surface` (`ef5ad3d`). Décision : taxonomie cascade + poseur déterministe + habillage agent ; Jevlike / régression en option. Spec initiale (T0). |
| 2026-09-20 | T1–T4 livrés : `illustration_pose_taxonomy.json` + `illustration_taxonomy.rs` (classify / pose / apply) ; `enrich_illustration_puppet` remplit un squelette vide avant les recettes ; tests proto jardinier assis, chat carnivore, abstention OOD ; squelette déjà écrit préservé. |
| 2026-09-20 | T4.5 : verrou taxonomique (plus d’écrasement recette espèce/personne) ; joints secondaires (`pipe_`/`seat_`/`garden_`) + contact `hold_pipe` ; review `skeleton_undressed` ; compose fusionne skeleton/contacts/bindings ; args outil exposent ces champs. |

Les entrées suivantes notent : changements de catalogue, résultats de tests
visuels (chemins d’artifacts), décisions d’architecture, abandons.

## 13. Références

- Branche de base Illustration : `cursor/illustration-surface`
- Proto / squelette : `crates/aos-proto/src/illustration.rs`
- Benchmarks image vs procédural : `benchmarks/illustration/README.md`
- Modèle Choice Jevlike (hors arbre Akasha) : `E:\rlcd_model`
- Analyse d’état module : canvas `illustration-module-state.canvas.tsx`
