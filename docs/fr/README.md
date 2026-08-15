# Akasha OS

**Langue :** [English](../../README.md) | Français

**Site :** [azerothl.github.io/akasha-os](https://azerothl.github.io/akasha-os/?lang=fr)

**Système d'exploitation agent-natif** — capacités, IPC sémantique, GPU
first-class, offline-first. La Preview tourne sur hôte Windows/Linux
(NVIDIA) ; la piste seL4 est séparée.

> Ce n'est **pas** encore un OS bootable. La Preview 0.1.2 est une application
> hôte installable pour testeurs.

## Pourquoi Akasha OS

La plupart des stacks agents sont des apps sur un OS généraliste. Akasha OS
traite agents, modèles, outils et mémoire comme des **services système**
avec capacités explicites, audit et politiques.

## Fonctionnalités Preview

Catalogue : [FEATURES.md](FEATURES.md).

| Domaine | Contenu |
|---------|---------|
| Chat / Sessions | Conversations persistées ; slash ; modèle par session |
| Mémoire | Faits long terme ; bootstrap mémoire d'abord |
| Notes | Humaines + agents (module WASM) |
| Agents | Goal, skills, outils, MCP ; timeline ; steer / pause |
| Modèles | Packs selon le GPU ; download / load ; local-only par défaut |
| Réseau | Recherche opt-in (Brave / DDG / Bing) + `web.browse` + fetch |
| Settings | Langue, trust, routage, défauts agent, moteur |
| Retour | Rapport local + issue GitHub |
| Mises à jour | Overlay non destructif depuis GitHub Releases |

## Prérequis

Windows 10/11 x64 **ou** Linux x64, GPU NVIDIA, ~4 Go disque.

## Démarrage rapide

1. Télécharger la Release GitHub.
2. `install.ps1` / `./install.sh`.
3. Lancer **Akasha OS Preview** (téléchargement des modèles au 1er run + tutoriel).

Voir [INSTALL.md](INSTALL.md), [FIRST-RUN.md](FIRST-RUN.md), [TESTER.md](TESTER.md).

## Documentation

| Doc | Description |
|-----|-------------|
| [STATUS.md](STATUS.md) | **État d'avancement** |
| [FEATURES.md](FEATURES.md) | **Fonctionnalités Preview livrées** |
| [../functional-specs.md](../functional-specs.md) | Specs fonctionnelles (EN) |
| [specs-fonctionnelles.md](specs-fonctionnelles.md) | Specs fonctionnelles (FR) |
| [I18N.md](I18N.md) | Convention multilingue |
| [../../CONTRIBUTING.md](../../CONTRIBUTING.md) | Contributions |

## Licence

Double licence AGPL-3.0-only + commerciale — voir le [README anglais](../../README.md).

## Statut

Voir **[STATUS.md](STATUS.md)**.
