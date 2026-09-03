# Akasha OS

<p align="center">
  <img src="../../branding/logo/logo.svg" width="96" height="96" alt="Akasha OS">
</p>

**Langue :** [English](../../README.md) | Français

**Site :** [azerothl.github.io/akasha-os](https://azerothl.github.io/akasha-os/?lang=fr)

**Système d'exploitation agent-natif** — capacités, IPC sémantique, GPU
first-class, offline-first. La Preview tourne sur hôte Windows/Linux
(NVIDIA ou CPU) et macOS Apple Silicon ; la piste seL4 est séparée.

<p align="center">
  <img src="../../branding/logo/tracks.svg" width="480" alt="MEMORY, CAPS, GPU, AGENTS">
</p>

> Ce n'est **pas** encore un OS bootable. La Preview 0.16.1 est une application
> hôte installable pour testeurs (Windows/Linux/macOS Apple Silicon ; NVIDIA
> optionnel sur Win/Linux via le chemin CPU du même zip).

## Pourquoi Akasha OS

La plupart des stacks agents sont des apps sur un OS généraliste. Akasha OS
traite agents, modèles, outils et mémoire comme des **services système**
avec capacités explicites, audit et politiques.

## Fonctionnalités Preview

Catalogue : [FEATURES.md](FEATURES.md).

| Domaine | Contenu |
|---------|---------|
| Chat / Sessions | Conversations persistées ; recherche, épingle, archive réversible ; slash (`/image`, `/speak`) ; modèle par session |
| Mémoire | Faits long terme ; bootstrap mémoire d'abord |
| Notes | Humaines + agents (module WASM) ; resync au boot après une update |
| Tâches | Module dual-surface + onglet Tasks |
| Agents | Goal, skills, outils, MCP, scheduler ; timeline et export Markdown des traces |
| Caps | Liste / révocation dans l'UI |
| Modèles | Packs selon le matériel (dont CPU) ; nouvelle tentative après erreur de chargement ; contrôles cache de préfixe/spéculation ; packs image/TTS optionnels ; métriques live TTFT / tok/s / VRAM |
| Canvas | Dessin vectoriel avec calques, grille, édition, exports PNG/SVG/JSON et validation géométrique globale des agents |
| Providers | Cloud OpenAI-compat + loopback (Ollama / vLLM / LM Studio) ; clés dans le vault |
| Réseau | Recherche opt-in (Brave / DDG / Bing) + `web.browse` + fetch |
| Settings | Langue, thème, densité, trust, routage, gpu/cpu/auto, défauts agent, moteur |
| Retour | Rapport local + issue GitHub |
| Dépannage | Diagnostic in-app ; rapport GitHub s'il y a des anomalies |
| Mises à jour | Overlay non destructif depuis GitHub Releases |

## Prérequis

Windows 10/11 x64, Linux x64 **ou** macOS Apple Silicon (pas Intel) ; GPU NVIDIA
recommandé sur Windows/Linux **ou** mode CPU-only (plus lent) ; ~8 Go libres recommandés.

## Démarrage rapide

1. Télécharger la Release GitHub.
2. `install.ps1` / `./install.sh`.
3. Lancer **Akasha OS Preview** (téléchargement des modèles au 1er run + tutoriel).

Voir [INSTALL.md](INSTALL.md), [FIRST-RUN.md](FIRST-RUN.md),
[TESTER.md](TESTER.md) (chemin de 15 minutes d’abord). Lieu de rencontre :
[community.md](community.md).

### Build depuis les sources

Toolchain, scripts de packaging et run local :
[INSTALL.md — Build depuis les sources](INSTALL.md#build-depuis-les-sources)
(anglais : [INSTALL.md — Build from source](../INSTALL.md#build-from-source)).

```powershell
cargo test --workspace
.\packaging\build-preview.ps1 -SkipModels -RequireCuda
$env:AOS_HOME = (Resolve-Path .)
cargo run -p aos-session --release
```

## Documentation

| Doc | Description |
|-----|-------------|
| [STATUS.md](STATUS.md) | **État d'avancement** |
| [FEATURES.md](FEATURES.md) | **Fonctionnalités Preview livrées** |
| [INSTALL.md](INSTALL.md) | Installation, mises à jour, build depuis les sources |
| [FIRST-RUN.md](FIRST-RUN.md) | Premier lancement |
| [../functional-specs.md](../functional-specs.md) | Specs fonctionnelles (EN) |
| [specs-fonctionnelles.md](specs-fonctionnelles.md) | Specs fonctionnelles (FR) |
| [paysage-concurrentiel.md](paysage-concurrentiel.md) | Paysage OS agentiques vs Akasha OS |
| [plan-evolutions.md](plan-evolutions.md) | Priorités d’évolution post-paysage (E1–E15) |
| [I18N.md](I18N.md) | Convention multilingue |
| [community.md](community.md) | Cohorte Preview (Discussions ; 3 Win + 1 Linux + 1 Mac) |
| [write-a-skill.md](write-a-skill.md) | Guide skill en dix minutes (MIT, `var/skills/`) |
| [write-a-module.md](write-a-module.md) | Premier module sans cargo (scaffold / caps) |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Comment tester, discuter, et (si PR) licence |

## Licence

**OS hôte** : double licence AGPL-3.0-only + commerciale.
**Extensions guest** ([ADR 0006](../../adr/0006-license-split.md)) : Apache-2.0
(`modules/`) et MIT (`skills/`, `community/`) — pas d’octroi commercial.
Détail : [README anglais](../../README.md).

## Statut

Cohorte Preview (PC) en cours. Gate : **3 Windows + 1 Linux + 1 macOS
Apple Silicon** sur le chemin de 15 minutes. Tables :
**[STATUS.md](STATUS.md)**.
