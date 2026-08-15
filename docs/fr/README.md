# Akasha OS (Agent OS)

**Langue :** [English](../../README.md) | Français

**Site :** [azerothl.github.io/akasha-os](https://azerothl.github.io/akasha-os/?lang=fr)

**Système d'exploitation agent-natif** — capacités, IPC sémantique, GPU
first-class, offline-first. La Preview tourne sur hôte Windows/Linux
(NVIDIA) ; la piste seL4 est séparée.

> Ce n'est **pas** encore un OS bootable. La Preview 0.1 est une application
> hôte installable pour testeurs.

## Pourquoi Agent OS

La plupart des stacks agents sont des apps sur un OS généraliste. Agent OS
traite agents, modèles, outils et mémoire comme des **services système**
avec capacités explicites, audit et politiques.

## Fonctionnalités Preview

| Domaine | Contenu |
|---------|---------|
| Chat / Sessions | Conversations parallèles persistées |
| Mémoire | Faits long terme |
| Notes | Humaines + agents (module WASM) |
| Agents | Boucle de goal, skills, outils, MCP optionnel |
| Réseau | Recherche / fetch en opt-in (offline par défaut) |
| Retour | Rapport local + issue GitHub |
| Mises à jour | Overlay non destructif depuis GitHub Releases |

## Prérequis

Windows 10/11 x64 **ou** Linux x64, GPU NVIDIA, ~4 Go disque.

## Démarrage rapide

1. Télécharger la Release GitHub.
2. `install.ps1` / `./install.sh`.
3. Lancer **Agent OS Preview** (téléchargement des modèles au 1er run + tutoriel).

Voir [INSTALL.md](INSTALL.md), [FIRST-RUN.md](FIRST-RUN.md), [TESTER.md](TESTER.md).

## Documentation

| Doc | Description |
|-----|-------------|
| [STATUS.md](STATUS.md) | **État d'avancement** |
| [../functional-specs.md](../functional-specs.md) | Specs fonctionnelles (EN) |
| [specs-fonctionnelles.md](specs-fonctionnelles.md) | Specs fonctionnelles (FR) |
| [I18N.md](I18N.md) | Convention multilingue |
| [../../CONTRIBUTING.md](../../CONTRIBUTING.md) | Contributions |

## Licence

Double licence AGPL-3.0-only + commerciale — voir le [README anglais](../../README.md).

## Statut

Voir **[STATUS.md](STATUS.md)**.
