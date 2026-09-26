# Carte d’architecture — hôte Preview

**Langue :** [English](../architecture.md) | Français

Diagramme interactif et graphe machine-readable de l’architecture **hôte
Preview** d’Akasha OS (daemons, bus, modules, plans trust / model / agent).

C’est le **scaffold Preview** (app hôte Windows / Linux / macOS), pas la
cible seL4 bare-metal. Voir [specs-techniques.md](specs-techniques.md) et
[ADR 0001](../../adr/0001-microkernel.md).

## Fichiers

| Fichier | Usage |
|---------|--------|
| [architecture-map.html](../architecture-map.html) | Carte interactive autonome (ouvrir dans un navigateur) |
| [architecture-graph.json](../architecture-graph.json) | `{ nodes, edges, flows }` pour agents et outillage |

Ouvrir le HTML en local (double-clic ou serveur de fichiers statiques). Aucun
build.

## Politique de release (obligatoire)

**Mettre à jour ce diagramme à chaque release Preview** (chaque tag `v*` /
passage packaging documenté dans [packaging-RELEASE.md](packaging-RELEASE.md)) :

1. Différencier daemons, modules, intents et flux majeurs depuis le tag
   précédent.
2. Rafraîchir `architecture-graph.json` (`nodes`, `edges`, `flows`).
3. Resynchroniser `architecture-map.html` pour qu’il embarque le même graphe
   (le HTML est autonome ; JSON et HTML restent alignés).
4. Mettre `meta.version` du JSON à la version de release.
5. Noter le rafraîchissement dans la checklist / notes de release.

Ne pas publier une release dont la carte décrit encore la disposition de
processus de la version précédente si daemons, modules ou flux principaux ont
changé.
