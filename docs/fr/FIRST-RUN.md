# Premier lancement — Akasha OS Preview 0.7.0

**Langue :** [English](../FIRST-RUN.md) | Français

> Date : 18/08/2026 · Preview **0.7.0**

**Ce n'est pas un OS bootable.** La Preview 0.7.0 tourne sur Windows ou Linux x64.
**NVIDIA est recommandé** ; le boot CPU-only fonctionne (plus lent).

Catalogue : [FEATURES.md](FEATURES.md).

## Avant de lancer

1. Driver NVIDIA (`nvidia-smi -L` OK), **ou** paquet / mode CPU-only.
2. ~8 Go libres (pack mid : 9B + embedding) ; moins pour le pack **cpu**.
3. Préférer `install.cmd` / `install.sh`, ou lancer `bin/aos-session`
   (synchronise vers `%LOCALAPPDATA%\AgentOS-Preview` /
   `~/.local/share/agentos-preview` pour que historique, mémoire et notes
   survivent aux nouvelles versions).
4. Sous Windows, si SmartScreen affiche **Éditeur inconnu** :
   **Informations complémentaires** → **Exécuter quand même** (Preview pas
   encore signée Authenticode).

## Setup modèles selon le matériel

Au premier lancement (pas encore de `var/models/installed.json`) :

1. Sonde GPU / RAM / disque → `var/run/hardware.json`.
2. Fenêtre **Choix des modèles** (auto-best selon le tier) :
   - **cpu** (pas de NVIDIA) : Qwen3.5-4B + Embedding 0.6B
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
| Mémoire | Faits long terme ; injection `mem.context` ; mémorisation auto opt-in depuis le chat (Settings) |
| Notes | Humaines + via agent (module WASM) |
| Agents | Goal, skills, outils, MCP ; **modèle** à la création ; **Détail** |
| Models | Liste / load / download ; bandeau si nouveaux packs |
| Audit | Événements signés ; tuer auditd (le superviseur le relance) |
| Réseau (barre latérale) | Opt-in `web.search` / `web.browse` / `net.fetch` |
| Settings | Langue, trust, routage, défauts agent, moteur de recherche, mémorisation auto |
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

- Pas de GPU → démarrage CPU-only (lent OK), ou installer un driver NVIDIA / paquet GPU.
- Setup annulé → relancer pour rouvrir le choix.
- Logs → `var/run/*.stderr.log`.
