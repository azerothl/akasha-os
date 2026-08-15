# Spécifications fonctionnelles — Agent OS

**Langue :** [English](../functional-specs.md) | Français

> Version : 0.3  
> Date : 15/08/2026  
> Statut : brouillon  
> Référence : `reflexion-agent-os.md`  
> Changements v0.3 : couverture Preview 0.1.2 — F-AGT-11, F-NET-01/02, F-UI-09 ; annexe §12.  
> Changements v0.2 : revue de complétude croisée avec `specs-techniques.md` — clarification du périmètre mono-utilisateur, ajout de F-BOOT-06, F-AGT-10, F-FS-05, F-SEC-07, F-SEC-08, précisions NFR-04 et glossaire.

---

## 1. Objectif produit

Concevoir un **operating system agent-natif** (ci-après **Agent OS**) permettant :

1. d'exécuter des **agents intelligents** avec une grande liberté d'action contrôlée ;
2. d'offrir une **expérience utilisateur** claire, transparente et interruptible ;
3. d'être **extensible** via modules/applications consommables par agents **et** humains ;
4. d'exécuter des **modèles d'IA locaux embarqués** dès le premier démarrage, et des **modèles distants** en complément ;
5. de **dispatcher intelligemment** les modèles lourds entre **RAM, GPU/NPU et disque** pour maximiser les performances sur du matériel hétérogène.

Agent OS n'est **pas** un simple runtime d'agents posé sur Linux/Windows : c'est un système dont les primitives (capacités, mémoire, scheduler, IPC, stockage) sont pensées pour des charges agentiques et d'inférence.

---

## 2. Périmètre

### 2.1 Inclus (v1)

- Runtime d'agents multi-agents concurrent
- Gestion unifiée des modèles locaux et distants
- Modèles locaux embarqués pour bootstrap / offline
- Dispatch des poids de modèles entre RAM / GPU-NPU / disque
- Modèle de sécurité par capacités
- Modules/apps extensibles (double surface agent + humain)
- UI duale (directe + conversationnelle)
- Audit, réversibilité, autonomie graduée
- Filesystem sémantique (enrichi, pas exclusif)

### 2.2 Hors périmètre (v1)

- Réécriture complète de tous les drivers matériels from scratch
- Compatibilité binaire native Windows/Linux (hors couche de compat éventuelle ultérieure)
- Marketplace public complet (prévu phase ultérieure ; registre local v1)
- Support formel de tous les backends cloud (priorité : API OpenAI-compatibles + backends locaux)
- **Multi-utilisateur simultané** (comptes séparés avec isolation OS complète) : v1 est **mono-utilisateur principal** ; plusieurs agents concurrents sont supportés, pas plusieurs comptes humains isolés
- Modèle économique de distribution des modules (paiement, licences) : le registre v1 est local et gratuit

### 2.3 Hypothèses

- Matériel cible v1 : x86_64 et/ou aarch64, avec ou sans GPU/NPU
- Au moins un support de stockage persistant (SSD recommandé)
- Réseau optionnel : le système doit démarrer et fonctionner **offline** avec les modèles embarqués
- **Un seul utilisateur humain principal** par instance Agent OS en v1 ; ce même utilisateur porte par défaut le rôle « Administrateur système ». La séparation stricte multi-comptes est une évolution future (voir roadmap technique)

---

## 3. Acteurs

| Acteur | Description |
|--------|-------------|
| **Utilisateur humain** | Personne qui interagit via UI directe ou conversationnelle, délègue des tâches, reprend la main, configure politiques |
| **Agent** | Entité logicielle autonome exécutée par le runtime, agissant via capacités déléguées |
| **Module / Application** | Paquet installable exposant outils (agents) et/ou UI (humains) |
| **Modèle d'IA** | LLM / VLM / embedding / autre modèle local ou distant consommé par le runtime |
| **Administrateur système** | Configure politiques globales, quotas, modèles par défaut, confiance |
| **Agent superviseur** | Agent système chargé de prioriser notifications, médiation multi-agents, garde-fous |

> Note de périmètre (v1) : l'acteur « Administrateur système » n'est pas un compte séparé — c'est le rôle étendu porté par défaut par l'utilisateur humain principal (cf. §2.3). La distinction redevient pertinente si le multi-utilisateur est introduit dans une version ultérieure.

---

## 4. Concepts clés

### 4.1 Capacité (capability)

Jeton non falsifiable, révocable, éventuellement temporaire, autorisant une action précise (ex. : lire le fichier X, appeler le modèle Y, ouvrir une socket vers Z). Toute action d'un agent passe par une capacité.

### 4.2 Intention (intent)

Demande structurée de haut niveau émise par un agent ou un humain, décomposée par le système en chaîne d'opérations bas niveau (syscalls sémantiques).

### 4.3 Contexte cognitif

État d'un agent : mémoire de travail, historique, capacités actives, tâches en cours. Sauvegardable / restaurable (context switch cognitif).

### 4.4 Backend de modèle

Fournisseur d'inférence :
- **Local embarqué** : livré avec l'OS, disponible offline
- **Local installé** : téléchargé/ajouté par l'utilisateur
- **Distant** : API réseau (cloud ou serveur privé)

### 4.5 Placement de modèle (Model Placement)

Stratégie de répartition des couches/poids d'un modèle entre :
- **VRAM GPU / NPU**
- **RAM système**
- **Disque** (offload / streaming des couches)

### 4.6 Module double-surface

Package unique exposant :
- un **schéma d'outils** pour les agents (function calling / IPC sémantique)
- une **interface de rendu** pour les humains

---

## 5. Exigences fonctionnelles

### 5.1 Démarrage et bootstrap

| ID | Exigence | Priorité |
|----|----------|----------|
| F-BOOT-01 | L'OS démarre et atteint un état utilisable **sans connexion réseau** | Must |
| F-BOOT-02 | Au premier démarrage, au moins un **modèle local embarqué** est disponible pour l'assistant système et l'initialisation | Must |
| F-BOOT-03 | L'utilisateur peut terminer l'onboarding (langue, préférences, politiques de confiance) via UI conversationnelle **ou** classique | Must |
| F-BOOT-04 | Si un GPU/NPU est détecté, le placement initial des modèles embarqués l'utilise automatiquement quand possible | Should |
| F-BOOT-05 | En cas d'échec de chargement GPU, fallback automatique CPU/RAM/disque sans bloquer le boot | Must |
| F-BOOT-06 | Le système peut être mis à jour (image système) avec rollback automatique en cas d'échec de démarrage post-update | Should |

### 5.2 Runtime d'agents

| ID | Exigence | Priorité |
|----|----------|----------|
| F-AGT-01 | Exécuter plusieurs agents en parallèle avec isolation par capacités | Must |
| F-AGT-02 | Créer, suspendre, reprendre, arrêter un agent | Must |
| F-AGT-03 | Sauvegarder et restaurer le contexte cognitif d'un agent (préemption) | Must |
| F-AGT-04 | Permettre la collaboration multi-agents via mémoire partagée contrôlée par capacités | Should |
| F-AGT-05 | Un agent ne peut effectuer que des actions pour lesquelles il détient une capacité valide | Must |
| F-AGT-06 | L'utilisateur peut interrompre ou rediriger (steer) un agent en cours d'exécution sans perdre le contexte utile | Must |
| F-AGT-07 | Chaque action significative d'agent est tracée dans un audit trail consultable | Must |
| F-AGT-08 | Les actions à effet de bord sont **réversibles par défaut** (transaction / snapshot / undo) quand techniquement possible | Must |
| F-AGT-09 | Autonomie graduée : le niveau de capacités d'un agent peut évoluer selon un score de confiance et des politiques utilisateur | Should |
| F-AGT-10 | Un agent superviseur système peut arbitrer les conflits de ressources ou de priorité entre agents concurrents (au-delà de la simple notification) | Should |
| F-AGT-11 | Avant les outils, le runtime classe le goal (`task.assess` simple/complex) et consulte d'abord la mémoire (`mem.bootstrap`) | Should |

### 5.3 Gestion des modèles d'IA

| ID | Exigence | Priorité |
|----|----------|----------|
| F-MDL-01 | Supporter des backends **locaux** et **distants** via une API unifiée | Must |
| F-MDL-02 | Livrer par défaut des **modèles locaux embarqués** (au minimum : un petit LLM instruct + un modèle d'embeddings) | Must |
| F-MDL-03 | Permettre l'ajout, la mise à jour, la suppression de modèles locaux | Must |
| F-MDL-04 | Permettre la configuration de backends distants (URL, clés, modèles disponibles) | Must |
| F-MDL-05 | Routage intelligent : choisir automatiquement local vs distant selon disponibilité, latence, coût, politique privacy, taille de tâche | Should |
| F-MDL-06 | Fonctionnement **offline complet** avec les seuls modèles locaux | Must |
| F-MDL-07 | L'utilisateur peut forcer un backend (local only / remote preferred / remote only) par agent ou globalement | Must |
| F-MDL-08 | Afficher l'état des modèles : chargé, partiellement offloadé, sur disque, distant, en erreur | Must |
| F-MDL-09 | Support multi-modèles simultanés (ex. : LLM + embeddings + VLM) | Must |
| F-MDL-10 | Quota et priorisation d'accès aux modèles entre agents (ex. budget de tokens ou de temps d'inférence par période, par agent ou par module) | Should |

### 5.4 Dispatch des modèles lourds (RAM / GPU / Disque)

| ID | Exigence | Priorité |
|----|----------|----------|
| F-PLC-01 | Le système peut **répartir les poids d'un modèle** entre VRAM (GPU/NPU), RAM système et disque | Must |
| F-PLC-02 | Placement **automatique** selon ressources disponibles, priorité des agents et politiques utilisateur | Must |
| F-PLC-03 | Placement **manuel** configurable (ex. : « layers 0-20 GPU, reste RAM », ou profils prédéfinis) | Should |
| F-PLC-04 | **Streaming / offload** des couches depuis le disque vers RAM/GPU à la demande pendant l'inférence | Must |
| F-PLC-05 | Préchargement (prefetch) intelligent des couches susceptibles d'être utilisées | Should |
| F-PLC-06 | Éviction dynamique : si un agent prioritaire a besoin de VRAM, déplacer/décharger des couches d'un modèle moins prioritaire | Must |
| F-PLC-07 | Support multi-GPU et multi-device (répartition inter-GPU) quand le matériel le permet | Could |
| F-PLC-08 | Métriques exposées : occupation VRAM/RAM/disque par modèle, hit rate cache couches, tokens/s, latence TTFT | Must |
| F-PLC-09 | Ne jamais bloquer le système entier si un modèle trop gros est demandé : dégradation contrôlée ou refus explicite avec alternative | Must |
| F-PLC-10 | Profils de placement prédéfinis : `latency` (max GPU), `balanced`, `memory-saver` (max offload disque), `cpu-only` | Should |

### 5.5 Mémoire agentique

| ID | Exigence | Priorité |
|----|----------|----------|
| F-MEM-01 | Mémoire de travail par agent (contexte court terme) | Must |
| F-MEM-02 | Mémoire long terme / épisodique avec recherche sémantique (embeddings) | Must |
| F-MEM-03 | Mémoire partagée inter-agents sous contrôle de capacités | Should |
| F-MEM-04 | Politique d'éviction sémantique (pertinence) en plus des politiques classiques (LRU) | Should |
| F-MEM-05 | L'utilisateur peut inspecter, éditer, effacer la mémoire d'un agent | Must |

### 5.6 Stockage et filesystem

| ID | Exigence | Priorité |
|----|----------|----------|
| F-FS-01 | Stockage hiérarchique classique (chemins) pour performance et clarté | Must |
| F-FS-02 | Adressage sémantique : retrouver des données par description / intention | Should |
| F-FS-03 | Snapshots / versions pour permettre undo des actions agents | Must |
| F-FS-04 | Isolation des espaces de données par capacités (pas d'accès ambiant) | Must |
| F-FS-05 | Chaque donnée peut porter une **classification de sensibilité** (ex. public / privé / secret), assignée par défaut (héritage dossier) et modifiable par l'utilisateur, utilisée par les politiques de routage et de partage | Must |

### 5.7 Modules et applications

| ID | Exigence | Priorité |
|----|----------|----------|
| F-MOD-01 | Installer / désinstaller / mettre à jour un module | Must |
| F-MOD-02 | Chaque module déclare un **manifeste double** : outils agents + UI humaine | Must |
| F-MOD-03 | Découverte dynamique des capacités d'un module (introspection de schéma) | Must |
| F-MOD-04 | Exécution sandboxée des modules (pas d'accès système hors capacités déclarées) | Must |
| F-MOD-05 | Agents et humains consomment le **même** module via des surfaces adaptées | Must |
| F-MOD-06 | Registre local de modules en v1 ; distribution réseau en phase ultérieure | Must |

### 5.7bis Extensions agent (skills / modules)

| ID | Exigence | Priorité |
|----|----------|----------|
| F-EXT-01 | Un agent peut créer une **skill** déclarative (markdown + outils) sous gouvernance trust | Must |
| F-EXT-02 | Un agent peut demander une capacité manquante via `cap.request` (hot-grant) | Must |
| F-EXT-03 | Un agent peut scaffolder / packager un module script (ext-rt) sans toolchain Rust | Must |
| F-EXT-04 | Un agent peut compiler un module Rust→WASM si la toolchain est présente (cap critique) | Should |
| F-EXT-05 | `module.install` exige une capacité critique + revue des caps (plus d'install anonyme) | Must |
| F-EXT-06 | Le catalogue d'outils (`module.describe` + skills) est injecté dans le prompt agent | Must |

### 5.8 Interface utilisateur

| ID | Exigence | Priorité |
|----|----------|----------|
| F-UI-01 | Mode **direct** : navigation, paramètres, gestion fichiers, gestion agents/modèles | Must |
| F-UI-02 | Mode **conversationnel** : dialogue avec assistant système et agents | Must |
| F-UI-03 | Les deux modes coexistent et opèrent sur les mêmes données/capacités | Must |
| F-UI-04 | Transparence du raisonnement : afficher pourquoi un agent a agi (niveaux de détail) | Must |
| F-UI-05 | Reprise de contrôle humaine à tout moment (pause / cancel / steer) | Must |
| F-UI-06 | Tableau de bord ressources : CPU, RAM, VRAM, disque, modèles chargés, agents actifs | Must |
| F-UI-07 | Notifications priorisées (agent superviseur), pas de spam | Should |
| F-UI-08 | Accessibilité de base (contraste, taille texte, navigation clavier) | Should |
| F-UI-09 | Préférences persistées (langue, routage, trust, réseau, défauts agent, moteur de recherche) éditables depuis Settings | Must |

### 5.9 Sécurité, privacy, confiance

| ID | Exigence | Priorité |
|----|----------|----------|
| F-SEC-01 | Toute action passe par le modèle de capacités | Must |
| F-SEC-02 | Révocation immédiate d'une capacité ou d'un agent | Must |
| F-SEC-03 | Politique privacy : interdire l'envoi de certaines données vers backends distants | Must |
| F-SEC-04 | Secrets (clés API) stockés chiffrés, jamais exposés en clair aux agents non autorisés | Must |
| F-SEC-05 | Audit trail non altérable par les agents applicatifs | Must |
| F-SEC-06 | Isolation entre agents non liés (pas de fuite de contexte) | Must |
| F-SEC-07 | Le système peut exiger une **confirmation humaine explicite** avant l'exécution d'une action classée sensible par la politique (distinct du simple audit après coup) | Must |
| F-SEC-08 | L'accès réseau sortant (egress) d'un agent ou d'un module est contrôlé par capacité explicite (hôte/domaine autorisé) ; refus par défaut hors backends modèles configurés | Must |

### 5.9bis Outils réseau (Preview)

| ID | Exigence | Priorité |
|----|----------|----------|
| F-NET-01 | Recherche web opt-in avec moteur sélectionnable (`auto` / Brave / DuckDuckGo / Bing), refusée en offline strict | Must |
| F-NET-02 | Lire une page HTML→texte sans exécuter de JavaScript (`web.browse`), sous la même politique d'egress | Should |

### 5.10 Observabilité et administration

| ID | Exigence | Priorité |
|----|----------|----------|
| F-OBS-01 | Logs système structurés | Must |
| F-OBS-02 | Métriques d'inférence et de placement de modèles | Must |
| F-OBS-03 | Vue administrateur : politiques globales, quotas, modèles par défaut | Must |
| F-OBS-04 | Export d'audit pour analyse externe | Should |

---

## 6. Parcours utilisateurs principaux

### 6.1 Premier démarrage (offline)

1. Boot de l'OS  
2. Chargement du modèle embarqué (placement auto selon matériel)  
3. Assistant système guide l'onboarding  
4. Création du profil utilisateur et politiques de base (privacy, autonomie agents)  
5. Bureau / shell prêt, agents système actifs  

### 6.2 Déléguer une tâche

1. L'utilisateur décrit une intention (UI conversationnelle ou formulaire)  
2. Le système crée/assigne un agent avec capacités minimales nécessaires  
3. L'agent planifie et exécute ; l'utilisateur voit le progrès et le raisonnement  
4. Actions sensibles demandent confirmation selon politique  
5. Résultat livré ; undo possible si applicable  

### 6.3 Ajouter un modèle local lourd

1. L'utilisateur importe ou télécharge un modèle (ex. 70B quantifié)  
2. Le **Model Placement Manager** analyse taille, VRAM, RAM, disque  
3. Proposition d'un profil de placement (`balanced` par défaut)  
4. Chargement progressif ; métriques visibles  
5. Le modèle devient sélectionnable par les agents  

### 6.4 Configurer un backend distant

1. L'utilisateur ajoute endpoint + credentials  
2. Test de connectivité et découverte des modèles  
3. Politique : quels agents / quelles données peuvent l'utiliser  
4. Routage automatique ou manuel selon préférences  

### 6.5 Installer un module

1. Sélection d'un module dans le registre local  
2. Revue des capacités demandées (équivalent permissions store)  
3. Installation sandboxée  
4. Outils visibles par les agents ; UI visible par l'humain  

---

## 7. Règles métier importantes

1. **Offline-first** : aucune fonction critique du shell/assistant ne doit dépendre du réseau.  
2. **Least privilege** : capacités minimales par défaut ; élargissement explicite ou par confiance progressive.  
3. **Privacy by default** : les données locales ne partent pas vers un backend distant sans politique l'autorisant.  
4. **Dégradation gracieuse** : manque de VRAM → offload RAM/disque ; manque de réseau → modèles locaux ; modèle indisponible → alternative ou message clair.  
5. **Réversibilité** : préférer les opérations transactionnelles pour tout effet de bord agent.  
6. **Une API modèles unifiée** : agents et UI ne voient pas la différence locale/distant sauf informations d'état exposées volontairement.

---

## 8. Exigences non fonctionnelles (vue produit)

| ID | Catégorie | Exigence |
|----|-----------|----------|
| NFR-01 | Perf | TTFT (time to first token) du modèle embarqué < 2s sur machine de référence (à définir en tech) après warm-up |
| NFR-02 | Perf | Le shell UI reste réactif (> 30 FPS / interactions < 100ms) même sous charge d'inférence |
| NFR-03 | Fiabilité | Crash d'un agent ou d'un backend modèle ne fait pas planter le noyau ni l'UI système |
| NFR-04 | Scalabilité | Supporter ≥ 32 agents légers concurrents, dont jusqu'à 8 flux d'inférence simultanés, sur la machine de référence (cf. specs-techniques.md §13) |
| NFR-05 | Sécurité | Pas d'escalade de privilège via un module sandboxé |
| NFR-06 | Privacy | Mode « local only » garanti auditables |
| NFR-07 | UX | Onboarding complétable en < 10 minutes offline |
| NFR-08 | Extensibilité | Ajout d'un module sans recompilation de l'OS |
| NFR-09 | Observabilité | Métriques placement + inférence disponibles en temps réel |
| NFR-10 | Portabilité | Builds cibles x86_64 et aarch64 (roadmap) |

---

## 9. Critères d'acceptation globaux (v1)

- [ ] Boot offline avec assistant conversationnel fonctionnel (modèle embarqué)
- [ ] Exécution multi-agents isolés par capacités
- [ ] Ajout d'un modèle local > taille VRAM et inférence réussie via offload RAM/disque
- [ ] Configuration d'un backend distant et bascule locale/distante
- [ ] Installation d'un module double-surface consommé par un agent et un humain
- [ ] Interrupt / undo d'une action agent visible dans l'UI
- [ ] Tableau de bord ressources et audit trail opérationnels
- [ ] Politique « local only » empêchant tout appel réseau modèles
- [ ] Une action classée sensible déclenche une demande de confirmation humaine bloquante avant exécution
- [ ] Un module sans capacité réseau explicite ne peut atteindre aucun hôte distant

---

## 10. Glossaire

| Terme | Définition |
|-------|------------|
| Agent OS | Système d'exploitation objet de cette spécification |
| Capability | Droit d'action unitaire délégué |
| Intent | Demande sémantique de haut niveau |
| Model Placement | Répartition des poids d'un modèle sur GPU/RAM/disque |
| Offload | Déplacement de couches modèle hors VRAM (vers RAM ou disque) |
| TTFT | Time To First Token |
| Double-surface | Module exposant API agent + UI humaine |
| Context switch cognitif | Sauvegarde/restauration de l'état complet d'un agent |
| Classe de sensibilité | Étiquette (public / privé / secret) portée par une donnée, utilisée par les politiques de routage et de partage |
| Confirmation requise | Mécanisme bloquant demandant une validation humaine explicite avant l'exécution d'une action sensible |
| Egress réseau | Trafic réseau sortant initié par un agent/module ; contrôlé par capacité dédiée |
| Score de confiance | Indicateur d'historique comportemental d'un agent utilisé pour graduer son autonomie (détail : specs-techniques.md §4.7) |

---

## 11. Documents liés

- `reflexion-agent-os.md` — réflexion fondatrice  
- `specs-techniques.md` — spécifications techniques  
- `FEATURES.md` — catalogue Preview 0.1.2 livrée  
- (futur) `adr/` — Architecture Decision Records  

---

## 12. Couverture Preview 0.1.2 (hôte)

Correspondance de cette spec avec la **Preview hôte installable** (pas l'OS bootable).
Détail : `FEATURES.md`.

| IDs | État Preview |
|-----|----------------|
| F-BOOT-01–05 | Sonde matériel + setup modèles + chat offline ; pas de fallback CPU |
| F-BOOT-06 | Overlay Release non destructif (apply au prochain lancement) |
| F-AGT-01–08, 11 | Boucle de goal, caps, audit, steer, `task.assess`, mémoire d'abord |
| F-AGT-09–10 | Trust low/medium + confirm ; superviseur relance auditd |
| F-MDL-01–09 | llama.cpp local + remote optionnel ; routage local_only / balanced |
| F-MEM-01–02, 05 | Working / épisodique / faits user ; onglet Mémoire |
| F-MOD / F-EXT | notes.aospkg, ext-rt, skills déclaratives, `cap.request` |
| F-UI-01–05, 09 | Double surface + panneau transparence + Settings |
| F-SEC-01–08 | Caps, confirm, egress deny-by-default, isolation auditd |
| F-NET-01–02 | Recherche multi-moteurs + `web.browse` |
