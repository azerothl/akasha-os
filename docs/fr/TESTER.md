# Protocole testeur — Akasha OS Preview

**Langue :** [English](../TESTER.md) | Français

Merci de tester la Preview. Objectif : installer **sans** `cargo` ni clone du
repo, exercer les parcours principaux, et envoyer un retour **depuis l'UI**.
Catalogue : [FEATURES.md](FEATURES.md).

## Avant de commencer

- Machine Windows ou Linux x64 (NVIDIA recommandé ; paquet / chemin CPU-only aussi OK)
- Installation : voir [INSTALL.md](INSTALL.md)
- Lancer **Akasha OS Preview** (`aos-session`)

Bannière attendue : *Preview sur Windows/Linux — ce n'est pas encore l'OS bootable*.

## Étapes (également dans l'onglet Scénarios)

### 1. Chat offline

- Terminer le **choix des modèles** (1er run) et le **tutoriel** (4 étapes).
- Onglet **Chat** : poser une question (ex. « Qu'est-ce qu'Akasha OS ? »).
- Combo **modèle de session** optionnel (onglet Models liste les offerings).
- Vérifier une réponse streamée **sans réseau**.
- Après une réponse, la barre latérale / **Models** doit afficher **TTFT** et **tok/s** (et VRAM sur GPU).

### 1b. Onglet Models

- Ouvrir **Models** : voir les entrées installées, **Download** une alternative si proposée.
- Bandeau « Models: … » si de nouveaux offerings correspondent au tier VRAM.
- Confirmer les métriques live pour le modèle chargé.

### 1c. CPU-only (optionnel)

- Sur une machine sans NVIDIA, ou avec Settings → Inférence → **CPU only** puis redémarrage :
  la Preview doit démarrer et le chat local doit fonctionner (lent OK).

### 2. Note humaine

- Onglet **Notes** → titre + contenu → **Créer**, puis **Lister**.

### 2b. Tasks (dual-surface)

- Onglet **Tasks** → créer une tâche.
- Lancer un agent avec les outils incluant `tasks.list` — il doit voir la même tâche.
- Optionnel : demander à l'agent un `tasks.create` ; rafraîchir l'onglet Tasks.

### 3. Note via agent

- Onglet **Agents** → créer un agent avec une tâche du type
  « crée une note intitulée cohorte avec le contenu hello ».
- L'agent utilise la convention `TOOL:` côté modèle.

### 4. Confirmation sensible

- Lorsqu'une confirmation apparaît en bandeau (action sensible) :
  **Refuser** une fois, puis **Accepter** une autre (ou la même rejouée).
- Fail-closed : timeout = refus.

### 5. Audit + caps + kill auditd

- Onglet **Audit** → **Rafraîchir** (événements signés).
- Onglet **Caps** : charger le détenteur `agent:<id>` pour un agent actif → voir les caps → **Révoquer** une cap non critique → confirmer une ligne d'audit.
- **Tuer aos-auditd** : le chat doit continuer ; le superviseur
  redémarre auditd en arrière-plan.

### 5b. Scheduler

- **Settings** → Schedules : créer un schedule avec intervalle **60s** et un goal court.
- Attendre un fire : un nouvel agent doit apparaître ; annuler le schedule pour qu'il ne fire plus.

### 6. Sessions parallèles (PC.6)

- Panneau **Sessions** (Chat) : créer 3 sessions, chatter dans chacune.
- Redémarrer Preview : les historiques doivent réapparaître.

### 7. Mémoire (PC.7 / P04)

- Onglet **Mémoire** : mémoriser « je préfère le français », **Lister**, puis
  « je préfère l'anglais ».
- Attendre un auto-lien (`supersedes` / `updates`) ; **Recall** doit préférer l'anglais.
- Éditer / supprimer / superséder ; cocher « Afficher supersédés ».
- Retour Chat : le prochain message doit utiliser ce contexte (`mem.context`).

### 7b. Coffre secrets (P04.3)

- **Settings → Secrets** : saisir une clé Brave (ou GitHub) → **Enregistrer**.
- Le magasin live est `vault.enc` (pas de YAML clair) ; **Lister** ne montre que les noms.
- Après first-run, `var/secrets/master.backend` vaut `keyring` ou `file`. Si
  `keyring`, `master.key` doit être absent.

### 7c. Revue de caps module (P04.4)

- Install sans `approved_caps` → confirmation listant les caps.
- **Accepter** → caps accordées ; **Refuser** → quarantaine / caps vides.

### 7d. Mémorisation auto depuis le chat (P05 / E14)

- **Settings** : **Mémorisation auto depuis le chat** est **activée** par défaut (décocher pour désactiver).
- Dans Chat, dire un fait durable ex. « Je préfère le français pour l’UI ».
- Après la réponse, le statut mentionne des fait(s) mémorisé(s) ; **Mémoire → Lister**
  affiche le fait avec un badge **`[chat]`**.
- Dire le contraire (« Je préfère l’anglais ») ; attendre auto-lien / `supersedes`.
- Coller une fausse clé (`sk-abcdefghijklmnopqrstuvwxyz1234` ou `ghp_…`) :
  elle ne doit **pas** apparaître comme fait (audit peut montrer `filtered`).
- Remettre l’option **off** : les tours suivants ne doivent plus écrire de faits.

### 7e. L’agent interroge l’utilisateur (`user.ask`)

- Lancer `/agent` avec une tâche qui demande une préférence (format, nom, choix).
- Quand l’agent pose une question, le placeholder devient « répondez à la
  question de l’agent » ; répondre dans le même fil (ou **Répondre** sur la
  carte s’il y en a plusieurs).
- L’agent reprend avec la réponse. Sans réponse ~10 min, la tâche continue
  (pas de blocage infini).

### 7f. Keyring OS (P06.3)

- Après Settings → Secrets **Enregistrer**, redémarrer : la clé fonctionne encore.
- `var/secrets/master.backend` vaut `keyring` ou `file`. Sous Windows, attendre
  `keyring` et pas de `master.key` lisible.

### 7g. Catalogue local signé (P06.4)

- **Settings → Catalogue local de modules** : notes / tasks / ext-rt listés.
- **Installer** un module listé → confirmation de revue de caps (comme 7c).
- Altérer un WASM packagé tout en gardant l'entrée catalogue doit refuser.

### 7h. Stop chat + Copier (P06.5)

- Pendant un stream, **Stop** interrompt la génération.
- **Copier** sur un message (ou le corps Dépannage / Retour) met le texte dans le presse-papiers.

### 8. Recherche web (PC.8 / PC.13)

- Case **Autoriser le réseau** (barre latérale) **désactivée** → **Rechercher**
  doit échouer (`offline_strict`).
- Activer le réseau → recherche (ex. « Akasha OS seL4 ») → résultats titre/URL.
- Settings → moteur : essayer `auto`, puis forcer `duckduckgo` ou `bing`.
- (Optionnel) clé Brave via **Settings → Secrets** (vault chiffré), pas un fichier clair.
  L'ancien `var/secrets/keys.yaml` est migré au boot.

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

## Hors scope Preview 0.6.0

- Boot seL4 / fer nu
- macOS
- Modèle 32B dans l'installeur
- Mise à jour automatique complète
- Génération audio/vidéo native
- Marketplace public / canaux messagerie / multi-GPU
