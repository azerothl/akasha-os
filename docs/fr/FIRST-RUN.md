# Premier lancement — Akasha OS Preview

**Langue :** [English](../FIRST-RUN.md) | Français

**Ce n'est pas un OS bootable.** Preview tourne sur Windows ou Linux x64
avec un GPU **NVIDIA**.

Catalogue : [FEATURES.md](FEATURES.md).

## Avant de lancer

1. Driver NVIDIA récent (`nvidia-smi -L` OK).
2. ~8 Go libres (pack mid : 9B + embedding).
3. Installer via `install.cmd` / `install.sh`, ou lancer `bin/aos-session`.
4. Sous Windows, si SmartScreen affiche **Éditeur inconnu** :
   **Informations complémentaires** → **Exécuter quand même** (Preview pas
   encore signée Authenticode).

## Setup modèles selon le matériel

Au premier lancement (pas encore de `var/models/installed.json`) :

1. Sonde GPU / RAM / disque → `var/run/hardware.json`.
2. Fenêtre **Choix des modèles** (auto-best selon le tier) :
   - **low** (&lt;10 Go VRAM) : Qwen3.5-4B + Embedding 0.6B
   - **mid** (10–20 Go) : Qwen3.5-9B + Embedding 0.6B
   - **high** (≥20 Go) : Qwen3 30B-A3B + Embedding 0.6B
3. Confirmer → téléchargement dans `share/models/`.
4. Démarrage des services + tutoriel egui.

Catalogue : `share/models/catalog-offerings.json`.

## Onglets utiles

| Onglet / surface | Usage |
|------------------|--------|
| Chat / Sessions | Modèle **par session** ; commandes slash (`/help`, `/agent`, `/notes`…) |
| Mémoire | Faits long terme ; injection `mem.context` |
| Notes | Humaines + via agent (module WASM) |
| Agents | Goal, skills, outils, MCP ; **modèle** à la création ; **Détail** |
| Models | Liste / load / download ; bandeau si nouveaux packs |
| Audit | Événements signés ; tuer auditd (le superviseur le relance) |
| Réseau (barre latérale) | Opt-in `web.search` / `web.browse` / `net.fetch` |
| Settings | Langue, trust, routage, défauts agent, moteur de recherche |
| Retour | Issue GitHub sur azerothl/akasha-os |
| Scénarios | Protocole cohorte ([TESTER.md](TESTER.md)) |

### Détail agent

Carte chat ou onglet Agents → **Détail** : état, badge simple/complex, sources,
timeline, Pause / Reprendre / Relancer / Kill / Steer.

### Settings

Préférences dans `var/run/preferences.json` (langue **en** / **fr**, routage,
trust, modèle agent, max steps, timeout, moteur, limites browse/fetch).

Mises à jour modèles : bandeau vert → Models → Download → redémarrer Preview.

## Réseau

Coupé par défaut (`offline_strict`). **Autoriser le réseau**, puis rechercher
(`auto` = Brave → DuckDuckGo → Bing) ou **Parcourir** une URL (HTML → texte).
Clé Brave optionnelle : `var/secrets/keys.yaml`.

## Dépannage

- Pas de GPU → driver NVIDIA.
- Setup annulé → relancer pour rouvrir le choix.
- Logs → `var/run/*.stderr.log`.
