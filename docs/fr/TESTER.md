# Protocole testeur — Akasha OS Preview

**Langue :** [English](../TESTER.md) | Français

Merci de tester la Preview. Objectif : installer **sans** `cargo` ni clone du
repo, exercer les parcours principaux, et envoyer un retour **depuis l'UI**.
Catalogue : [FEATURES.md](FEATURES.md).

## Avant de commencer

- Machine Windows ou Linux x64 + GPU NVIDIA (`nvidia-smi` OK)
- Installation : voir [INSTALL.md](INSTALL.md)
- Lancer **Akasha OS Preview** (`aos-session`)

Bannière attendue : *Preview sur Windows/Linux — ce n'est pas encore l'OS bootable*.

## Étapes (également dans l'onglet Scénarios)

### 1. Chat offline

- Terminer le **choix des modèles** (1er run) et le **tutoriel**.
- Onglet **Chat** : poser une question ; combo **modèle de session** optionnel.
- Vérifier une réponse streamée **sans réseau**.

### 1b. Onglet Models

- Lister / **Download** des offerings ; bandeau vert si nouveaux packs pour le tier VRAM.

### 2. Note humaine

- Onglet **Notes** → titre + contenu → **Créer**, puis **Lister**.

### 3. Note via agent

- Onglet **Agents** → créer un agent avec une tâche du type
  « crée une note intitulée cohorte avec le contenu hello ».
- L'agent utilise la convention `TOOL:` côté modèle.

### 4. Confirmation sensible

- Lorsqu'une confirmation apparaît en bandeau (action sensible) :
  **Refuser** une fois, puis **Accepter** une autre (ou la même rejouée).
- Fail-closed : timeout = refus.

### 5. Audit + kill auditd

- Onglet **Audit** → **Rafraîchir** (événements signés).
- Bouton **Tuer aos-auditd** : le chat doit continuer ; le superviseur
  redémarre auditd en arrière-plan.

### 6. Sessions parallèles (PC.6)

- Panneau **Sessions** (Chat) : créer 3 sessions, chatter dans chacune.
- Redémarrer Preview : les historiques doivent réapparaître.

### 7. Mémoire (PC.7)

- Onglet **Mémoire** : mémoriser un fait (« je préfère le français »), **Recall**.
- Revenir au Chat : le prochain message doit pouvoir s'appuyer sur ce contexte
  (injection `mem.context` avant infer).

### 8. Recherche web (PC.8 / PC.13)

- Case **Autoriser le réseau** (barre latérale) **désactivée** → **Rechercher**
  doit échouer (`offline_strict`).
- Activer le réseau → recherche (ex. « Akasha OS seL4 ») → résultats titre/URL.
- Settings → moteur : essayer `auto`, puis forcer `duckduckgo` ou `bing`.
- (Optionnel) clé Brave dans `var/secrets/keys.yaml` :
  `brave_search_api_key: "…"` — sinon DuckDuckGo / Bing HTML.

### 8b. Parcourir une page (PC.13)

- Réseau ON : coller une URL → **Parcourir** (`web.browse`).
- Attendre titre + texte extrait (sans JavaScript). Comparer avec
  **Télécharger URL** (`net.fetch`), qui enregistre le fichier brut sous
  `/downloads`.

### 9. Téléchargement + génération fichiers (PC.9)

- Avec réseau ON : coller une URL image → **Télécharger URL** → fichier sous
  `/downloads` (`var/storage/data/downloads/`).
- **Générer fichier** : format `pdf` ou `png`, chemin `/downloads/test.pdf`,
  contenu texte → **Ouvrir downloads**.

### 10. Retour depuis l'UI

- Onglet **Retour** (ou bouton **Signaler**) :
  - titre, catégorie (bug / ux / perf / security), sévérité, texte
  - case **Créer une issue GitHub** (cochée par défaut, sauf security)
  - **Envoyer le retour**
- Une copie locale est écrite dans `var/feedback/`.
- Une issue (ou le formulaire GitHub prérempli) s'ouvre sur
  [azerothl/akasha-os](https://github.com/azerothl/akasha-os/issues).
  Avec un compte GitHub, validez **Submit new issue**.

Les rapports **security** ne sont **pas** publiés.

**Aucun envoi réseau automatique** (hors actions explicites : PC.8–9, browse,
et envoi de retour GitHub).

### 11. Settings (PC.12)

- **Settings** : basculer en ↔ fr ; changer le modèle agent / max steps.
- Redémarrer Preview : les préférences dans `var/run/preferences.json` doivent
  persister.

### 12. Transparence agent (PC.11)

- Lancer un agent (onglet Agents ou `/agent` dans le Chat) avec une tâche courte.
- Ouvrir **Détail** : timeline, sources si recherche/browse, Pause puis Reprendre
  (ou Steer une nouvelle directive).
- Une tâche complexe affiche le badge **complex** (`task.assess`) et peut
  spawner un sous-agent (planner).

### 13. Notes après update + Dépannage (0.2.0)

- Après une install par-dessus une Preview précédente, ouvrir **Notes**, créer
  une note, puis la relire depuis la liste. Le WASM empaqueté doit correspondre
  à cette release.
- **Dépannage** (Aide / barre latérale) : collecte un diagnostic (NVIDIA, home,
  logs). S'il y a des anomalies, un rapport GitHub peut s'ouvrir.

## Critères de succès (équipe)

- 3 testeurs Windows + 1 Linux suivent ce protocole sans toolchain Rust
- Au moins un fichier `var/feedback/fb-*.json` exploitable par retour
- Gates PC.6–PC.9 et PC.11–PC.13 cochés sur au moins une machine

## Hors scope Preview 0.2.0

- Boot seL4 / fer nu
- macOS, CPU-only
- Modèle 32B dans l'installeur
- Mise à jour automatique complète
- Génération audio/vidéo native
