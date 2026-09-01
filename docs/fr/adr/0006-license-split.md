# ADR 0006 : Split de licence communauté (noyau vs extensions)

**Langue :** [English](../../../adr/0006-license-split.md) | Français

> Date : 01/09/2026 · Statut : **acceptée**

## Contexte

Akasha OS est en double licence : **AGPL-3.0-only** (`LICENSE`) et une offre
**commerciale** (`LICENSE-COMMERCIAL.md`). [CONTRIBUTING.md](../CONTRIBUTING.md)
exigeait que chaque pull request dans ce dépôt (1) place le travail sous
AGPL-3.0-only et (2) accorde à Loïc Peaudecerf une licence irrévocable de le
redistribuer sous les termes commerciaux.

Cet octroi entrant est nécessaire pour que l’**OS hôte** (daemons, caps, UI,
packaging) reste en double licence. Il est toxique pour un écosystème
d’extensions : l’auteur d’un skill Markdown ou d’un `.aospkg` dual-surface
n’accordera pas un droit SaaS propriétaire pour partager une recette.

L’Horizon 0 traite déjà une Discussion GitHub comme **hors** CLA. Ce n’est
pas suffisant. Les auteurs ont besoin d’un chemin git sans octroi commercial,
sans casser la double licence du noyau.

Les modules guest ne parlent à l’hôte que via `host_call` (`modules/sdk`).
Ce sont des œuvres séparées qui consomment une ABI à caps. Ils ne doivent
pas devenir des dérivés AGPL du seul fait de tourner sur la Preview.

Cette ADR **ne** crée **pas** de marketplace public (toujours hors Preview ;
E10 = catalogue signé plus tard). Elle ne relicencie pas l’hôte.

## Options

### A — Statu quo (toute PR = AGPL + octroi commercial)

La double licence reste triviale. Pas de skills ni modules tiers. Les
Discussions restent le seul partage sûr.

### B — Second dépôt seulement (noyau ici, toutes les extensions ailleurs)

Frontière juridique nette. Prématuré à l’échelle actuelle (un mainteneur,
cohorte Preview encore ouverte). Un dépôt catalogue reste possible plus
tard ; il ne doit pas être le seul moyen de partager un skill ce mois-ci.

### C — Split par chemins dans ce dépôt (retenu)

Les chemins noyau/hôte restent AGPL + octroi commercial entrant. Le SDK
guest et les extensions communautaires sont sous licences permissives
**sans** cet octroi. Un dépôt catalogue ultérieur, s’il existe, suit les
mêmes règles d’extension.

### D — Relicencier tout l’OS en MIT/Apache

Élargirait le vivier noyau et détruirait la double licence commerciale de
l’hôte. Rejeté.

## Décision

**Option C.** L’hôte est l’OS copyleft ; les extensions guest sont de
l’userspace dont les auteurs choisissent la licence.

| Artefact | Licence | Octroi commercial entrant (CLA) |
|----------|---------|----------------------------------|
| OS hôte : `crates/`, `packaging/`, `website/`, UI hôte, daemons, `vm/`, `demo/` | AGPL-3.0-only + offre commerciale | **Oui** — chaque PR |
| Docs qui décrivent l’hôte (`docs/`, `adr/`, README / NOTICE / licences racine) | Même régime que l’hôte | **Oui** |
| SDK guest : `modules/sdk` | **Apache-2.0** | **Non** |
| Modules guest first-party : `modules/notes`, `modules/tasks`, `modules/canvas`, `modules/ext-rt` (et leurs `.aospkg`) | **Apache-2.0** | **Non** (après relicence) |
| Skills first-party livrés : `skills/`, `share/skills/` | **MIT** | **Non** (après relicence) |
| Extensions communautaires : `community/` (skills, `.aospkg`, exemples) | Licence OSI de l’auteur, **défaut MIT** | **Non** |
| Discussion GitHub / Feedback in-app | n/a | **Non** |
| Futur dépôt catalogue signé (E10) | Index Apache-2.0 ; chaque paquet garde sa licence | **Non** |

Le titulaire du copyright (Loïc Peaudecerf) **autorise** la relicence du SDK
guest, des modules guest first-party et des skills livrés selon le tableau.
Les identifiants SPDX et les champs `license` des `Cargo.toml` de ces arbres
sont Apache-2.0 / MIT depuis le suivi qui a posé `LICENSE-APACHE`,
`LICENSE-MIT` et `community/`.

### Pourquoi Apache-2.0 pour le SDK (et les modules first-party)

Le SDK est l’ABI contre laquelle les auteurs compilent. Apache-2.0 donne un
octroi de brevets explicite et ne copyleft pas un module tiers. Les modules
first-party sont les modèles que les gens copieront ; les laisser en AGPL
piégerait « mon premier module » sous le CLA noyau.

Les skills livrés sont des recettes Markdown. MIT est le défaut le moins
frictionnel pour un texte que l’on fork.

### Pourquoi l’hôte reste AGPL + CLA

La licence commerciale est une **alternative** à l’AGPL pour les forks
propriétaires et le SaaS fermé de l’OS. Le CLA entrant sur les chemins hôte
est le seul moyen de redistribuer les rustines noyau tierces sur cette voie.
Cette ADR ne l’affaiblit pas.

### Guest vs hôte (pas une œuvre dérivée par seule exécution)

Un module qui n’utilise que l’ABI `host_call` publiée, des caps déclarées et
les widgets `declarative_ui` fermés **n’est pas** un dérivé AGPL de l’hôte
du seul fait d’être installé sur la Preview. Incorporer un module *dans* le
binaire hôte ou vendor `crates/` est un autre cas ; ça reste AGPL.

## Conséquences

### Politique (effective dès que CONTRIBUTING cite cette ADR)

- Une PR qui ne touche que des arbres Apache/MIT **n’accorde pas** la
  licence commerciale. Le contributeur certifie qu’il a le droit de
  soumettre et place le travail sous la licence de cet arbre.
- Une PR qui touche des arbres AGPL reste AGPL + octroi commercial, comme
  aujourd’hui.
- Une PR mixte est refusée ou découpée : fichiers AGPL d’un côté, fichiers
  permissifs de l’autre.
- Une Discussion Show and tell reste le partage zéro-CLA même après
  `community/`.
- La marque **Akasha OS** reste réservée. Un module ou fork communautaire
  ne prend pas ce nom de produit (`LICENSE-COMMERCIAL.md` §3).

### Suivi d’implémentation

1. ~~Ajouter `LICENSE-APACHE` / `LICENSE-MIT` ; Apache-2.0 sur les `Cargo.toml` guest ; SPDX.~~
2. ~~Marquer les `skills/*/SKILL.md` livrés comme MIT.~~
3. ~~Créer `community/` avec un README (défaut MIT, pas le CLA hôte).~~
4. ~~Réécrire [CONTRIBUTING.md](../CONTRIBUTING.md) (et `docs/fr/CONTRIBUTING.md`).~~
5. Zip Preview honnête : inclure `LICENSE-APACHE` et `LICENSE-MIT` à côté de
   `LICENSE` ; notes/tasks bundlés Apache ; binaires hôte AGPL. `NOTICE`
   décrit les deux couches.
6. Un catalogue réseau signé (E10) peut venir plus tard ; il n’exige pas
   l’octroi commercial. La revue de caps à l’install est inchangée.

### Hors scope

- Store public, paiements, ou clone de ClawHub
- Relicencier `crates/` ou abandonner l’offre commerciale
- Traiter les serveurs MCP ajoutés par l’utilisateur comme des œuvres
  Akasha OS
- Licence seL4 / fer nu (même double licence hôte lorsque ce code vit sous
  `crates/` ou `vm/` comme partie de l’OS)

## Notes

`docs/technical-specs.md` listait un placeholder `adr/0006-wasm-modules.md`.
Ce fichier n’a jamais été écrit. **0006 est ce split de licence.** Une ADR
ABI WASM future, si besoin, prend le prochain numéro libre.
