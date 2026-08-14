# Premier lancement — Agent OS Preview

**Langue :** [English](../FIRST-RUN.md) | Français

**Ce n'est pas un OS bootable.** Preview tourne sur Windows ou Linux x64
avec un GPU **NVIDIA**.

## Avant de lancer

1. Driver NVIDIA récent (`nvidia-smi -L` OK).
2. ~4 Go libres (binaires + modèles GGUF téléchargés au premier run).
3. Installer via `install.ps1` / `install.sh`, ou lancer `bin/aos-session`.

## Ce que fait le premier lancement

1. Vérifie NVIDIA et l'espace disque.
2. Télécharge les modèles Qwen2.5 (3B + 0.5B) dans `share/models/` s'ils
   manquent (réseau requis **une fois**).
3. Démarre les services et ouvre l'UI egui.
4. Affiche le **tutoriel** (onboarding) : langue, confiance, tour des onglets.

## Ce que vous pouvez faire

| Onglet | Usage |
|--------|--------|
| Chat / Sessions | Conversations parallèles persistées |
| Mémoire | Faits long terme (remember / recall) |
| Notes | Notes humaines + via agent |
| Agents | Tâches avec skills / outils |
| Réseau (case latérale) | Opt-in pour recherche web / téléchargements |
| Retour | Issue GitHub sur azerothl/akasha-os |
| Scénarios | Protocole cohorte (voir TESTER.md) |

Par défaut le réseau in-app est **coupé**. Les mises à jour logicielles
passent par GitHub Releases (bandeau dans l'UI) sans effacer `var/`.

## Dépannage rapide

- Pas de GPU → installer le driver NVIDIA (pas de mode CPU en 0.1).
- Modèle manquant → laisser le premier run télécharger, ou copier les GGUF
  dans `share/models/`.
- Logs des daemons → `var/run/*.stderr.log` (bouton Dépannage dans l'UI).
