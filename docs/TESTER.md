# Protocole testeur — Agent OS Preview 0.1

Merci de tester la Preview. Objectif : installer **sans** `cargo` ni clone du
repo, exercer les parcours principaux, et envoyer un retour **depuis l'UI**.

## Avant de commencer

- Machine Windows ou Linux x64 + GPU NVIDIA (`nvidia-smi` OK)
- Installation : voir [INSTALL.md](../INSTALL.md)
- Lancer **Agent OS Preview** (`aos-session`)

Bannière attendue : *Preview sur Windows/Linux — ce n'est pas encore l'OS bootable*.

## Étapes (également dans l'onglet Scénarios)

### 1. Chat offline

- Terminer l'onboarding (langue, `local_only`, confiance basse).
- Onglet **Chat** : poser une question (ex. « Qu'est-ce qu'Agent OS ? »).
- Vérifier une réponse streamée **sans réseau**.

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

### 6. Retour depuis l'UI

- Onglet **Retour** (ou bouton **Signaler**) :
  - titre, catégorie (bug / ux / perf / security), sévérité, texte
  - **Enregistrer le retour**
- Ouvrir le dossier `var/feedback/` et joindre le paquet à une issue GitHub
  ou au canal cohorte.

**Aucun envoi réseau automatique.**

## Critères de succès (équipe)

- 3 testeurs Windows + 1 Linux suivent ce protocole sans toolchain Rust
- Au moins un fichier `var/feedback/fb-*.json` exploitable par retour

## Hors scope Preview 0.1

- Boot seL4 / fer nu
- macOS, CPU-only
- Modèle 32B dans l'installeur
- Mise à jour automatique
