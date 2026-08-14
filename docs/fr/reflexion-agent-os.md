# Réflexion : un OS "agent-native" conçu from scratch

**Langue :** [English](../vision.md) | Français

> Date : 11/08/2026
> Contexte : réflexion sur la faisabilité d'un operating system construit dès le départ autour des agents (IA), sans base Windows/Linux, offrant une grande liberté aux agents tout en restant performant, agréable pour l'utilisateur, et extensible via des modules/applications accessibles aux agents comme aux humains.

---

## Cadrage initial

Avant de lister des pistes, il faut trancher une question de fond : **qu'est-ce qui doit vraiment être réinventé, et qu'est-ce qui ne doit pas l'être ?** Repartir de zéro sur les drivers, l'ordonnancement bas niveau, le TCP/IP stack, etc. est un projet de plusieurs centaines d'ingénieurs-années (voir seL4, Fuchsia/Zircon, Redox). La vraie innovation "agent-first" ne se situe pas dans le bas niveau, mais dans **la couche entre le noyau et les applications** : comment le système modélise l'intention, la mémoire, la confiance et les capacités des agents. C'est là qu'il faut concentrer l'énergie.

### Travaux de recherche existants à connaître (ne pas réinventer la roue)

- **AIOS (LLM Agent Operating System)** — Rutgers/agiresearch, papier COLM 2025 (arXiv:2403.16971) : noyau qui gère scheduling, context switch, mémoire, stockage et outils pour des agents LLM, avec jusqu'à 2,1x d'accélération vs exécution naïve. Repo : github.com/agiresearch/AIOS
- **LSFS** (LLM-based Semantic File System, ICLR 2025) : système de fichiers piloté par prompts plutôt que par commandes. arXiv:2410.11843
- **A-MEM** : mémoire agentique long terme. arXiv:2502.12110
- **Cerebrum** : SDK agent découplé du kernel (repo agiresearch/Cerebrum).
- **LiteCUA** : architecture "computer-use" avec VM Controller + serveur MCP en sandbox. arXiv:2505.18829
- Références OS classiques pertinentes : **seL4** (microkernel formellement vérifié, capability-based), **Fuchsia/Zircon** (Google, capability-based, component framework), **Genode OS Framework** (le plus proche conceptuellement : composants isolés, capacités explicites, extensible du micro-embarqué au desktop), **Redox OS** (microkernel en Rust).

Ces projets restent au-dessus de Linux — ce qui est révélateur : même en visant un "agent OS", personne ne réinvente le bas niveau. Recommandation : même posture, avec ambition d'aller plus loin sur les couches spécifiques agentiques.

---

## 1. Architecture noyau : capacités, pas permissions ambiantes

Le principe le plus important pour une "totale liberté" **sûre** des agents n'est pas l'absence de contrôle, mais un modèle de **sécurité par capacités** (capability-based security), comme seL4, Fuchsia/Zircon, ou Genode.

- Un agent ne "peut tout faire" que dans la limite des capacités qu'on lui délègue (fichier X, réseau Y, exécution de tel outil) — chaque capacité est un token non falsifiable, révocable, avec durée de vie.
- Pas de permissions globales style Unix (uid/gid) : trop grossier pour un agent autonome qui agit vite et en masse.
- Ça donne paradoxalement *plus* de liberté opérationnelle, car on peut octroyer des capacités larges à un agent de confiance sans risquer une compromission totale du système — et tout est auditable/révocable a posteriori.
- Microkernel plutôt que monolithique : isolation forte entre kernel, drivers, et "agent runtime", pour que la mémoire/sécurité des agents ne puisse pas planter le système.

## 2. "Syscalls sémantiques" plutôt que syscalls POSIX

L'agent ne devrait pas dialoguer avec le noyau en `open()/read()/write()`, mais en **intentions structurées** que le noyau traduit en primitives bas niveau.

- Une couche d'appel système sémantique : `intent("résume ce document et notifie l'utilisateur si urgent")` se décompose en une chaîne d'appels (lecture fichier, inference LLM, notification) orchestrée par le noyau, pas par l'agent lui-même.
- Standardiser ce protocole comme le fait **MCP (Model Context Protocol)** aujourd'hui pour les outils — mais en en faisant le protocole IPC natif du système, pas une couche applicative ajoutée après coup. Chaque app/module expose ses capacités via un schéma typé découvrable dynamiquement (introspection), consommé aussi bien par agents que par UI humaine.
- Le filesystem sémantique (LSFS) est une bonne piste : la donnée est adressée par description/intention, avec une couche POSIX classique en dessous pour la performance et la compatibilité (ne pas jeter la hiérarchie fichiers, l'enrichir).

## 3. Scheduler conscient de la nature des charges agentiques

Le vrai goulot de performance n'est presque jamais le CPU/kernel — c'est la **latence d'inférence LLM** (I/O-bound, pas CPU-bound) et l'accès GPU/NPU. Le scheduler doit être pensé pour ça dès le départ :

- Ordonnancement asynchrone/coopératif (green threads) où un agent en attente d'un token LLM libère immédiatement le cœur logique — pas de blocage façon thread OS classique.
- **Batching natif des requêtes d'inférence** au niveau noyau : si 20 agents appellent le même modèle en 50ms, le noyau doit pouvoir batcher côté GPU plutôt que laisser chaque agent faire une requête isolée (gain énorme constaté par vLLM, TGI, etc. — à faire au niveau OS plutôt que dans chaque appli).
- Le GPU/NPU doit être un **citoyen de première classe du scheduler**, pas un périphérique secondaire piloté par un driver applicatif — c'est l'inverse de Linux historiquement.
- Notion de "context switch cognitif" : sauvegarder/restaurer l'état complet d'un agent (contexte, mémoire de travail, capacités actives) comme on sauvegarde des registres CPU — nécessaire pour la préemption équitable entre agents concurrents.
- Priorisation par confiance/criticité plutôt que simple round-robin (un agent qui gère un paiement n'a pas la même priorité qu'un agent de veille).

## 4. Mémoire multi-niveaux (pas juste RAM/swap)

Un OS agent-natif a besoin d'une hiérarchie mémoire spécifique :
- **Mémoire de travail** (contexte court terme, fenêtre de contexte du LLM) — gérée comme un cache rapide.
- **Mémoire épisodique/long terme** — base vectorielle native (embeddings), avec politique d'éviction *sémantique* (pertinence) plutôt que LRU classique. A-MEM est une bonne référence de départ.
- **Mémoire partagée inter-agents** avec contrôle d'accès par capacités — pour la collaboration multi-agents sans fuite de contexte non désirée.

## 5. Sandboxing universel des modules/apps : WASM/WASI

Pour l'extensibilité (modules/apps consommables par agents ET humains) :
- Format de package universel basé sur **WASM/WASI** : portable, sandboxé par construction, exécutable côté serveur ou edge, avec un modèle de capacités qui colle exactement au modèle noyau (pas d'accès syscall implicite).
- Chaque module expose un **manifeste double** : (a) un schéma d'outils/fonctions consommable par les agents (function calling / MCP-like), (b) une interface de rendu pour les humains (UI déclarative). Même artefact, deux surfaces de consommation.
- Marketplace/registre avec signature et attestation des capacités demandées (comme un store mobile, mais avec des capacités bien plus granulaires et vérifiables formellement si possible).
- Versionning et compatibilité : les agents doivent pouvoir découvrir dynamiquement les capacités d'un module (introspection de schéma) sans redéploiement du système.

## 6. Sécurité et confiance : le vrai sujet critique

"Liberté totale" est le point le plus dangereux du projet. Recommandations concrètes :
- **Réversibilité par défaut** : toute action d'un agent ayant un effet de bord (fichier, réseau, achat) doit être conçue pour être annulable (transactions, snapshots type ZFS/btrfs, undo log) — le vrai frein à la confiance n'est pas la capacité, c'est l'irréversibilité.
- **Audit trail complet et lisible** : chaque décision d'agent (prompt, capacité utilisée, résultat) tracée nativement au niveau noyau, pas en option applicative.
- **Autonomie graduée** : un agent gagne des capacités plus larges avec un historique de confiance (comme un système de "credit score" comportemental), plutôt qu'un accès total dès l'installation.
- Isolation stricte entre agents non liés (pas de mémoire partagée implicite) pour éviter la contamination de prompt/contexte entre tâches différentes.

## 7. Expérience utilisateur humaine

Le piège classique d'un OS "pour agents" est d'oublier l'humain. Pistes :
- **Double surface d'interaction** : mode direct (GUI classique, clic/clavier) et mode conversationnel/délégué, coexistant sur les mêmes données/capacités — l'utilisateur choisit son niveau d'implication à tout moment.
- **Transparence du raisonnement** : la UI doit pouvoir montrer *pourquoi* un agent a fait quoi (chain of custody des décisions), sans noyer l'utilisateur — niveaux de détail configurables.
- **Interruption/steering en temps réel** : possibilité de reprendre la main à tout moment sur une tâche déléguée sans perdre le contexte déjà construit.
- Notifications intelligentes priorisées par un "agent superviseur" plutôt que la cacophonie actuelle des OS classiques.

## 8. Approche réaliste de mise en œuvre (roadmap)

Compte tenu du coût de réinvention d'un vrai bas niveau :

1. **Phase 1 — Prototype de la couche agentique** sur un noyau existant (Linux, ou microkernel type seL4/Redox pour la sécurité) : implémenter le modèle de capacités, le scheduler agent-aware, le protocole IPC sémantique, le sandboxing WASM. C'est essentiellement ce que fait AIOS, mais on peut aller plus loin sur les capacités et le sandboxing.
2. **Phase 2 — Composants système natifs** : filesystem sémantique, mémoire vectorielle native, gestion GPU/NPU en first-class citizen du scheduler.
3. **Phase 3 — Vrai noyau dédié** seulement si les phases précédentes démontrent des limites structurelles du noyau hôte (latence IPC, isolation insuffisante) — probablement basé sur un microkernel capability-based en Rust (inspiration seL4/Genode/Redox) plutôt que partir totalement à blanc.

## Points de vigilance

- Ne pas confondre "liberté de l'agent" et "absence de contrôle" — le modèle par capacités donne les deux à la fois (liberté d'action large + traçabilité/révocabilité).
- Le vrai risque de perf n'est pas le noyau mais l'inférence — concevoir le scheduler autour de ça dès le premier jour, pas en après-coup.
- Réinventer les drivers matériels (Wi-Fi, GPU, USB...) from scratch est un puits sans fond ; s'appuyer sur des couches d'abstraction matérielles existantes (via un hyperviseur léger ou un microkernel qui expose des drivers en espace utilisateur) est réaliste, réinventer ACPI/PCIe ne l'est pas.

---

## Pistes à creuser plus tard — état d'avancement

> Mise à jour (v0.2 des specs) suite à la revue de complétude de `specs-fonctionnelles.md` et `specs-techniques.md`.

| Piste initiale | Statut | Référence |
|---|---|---|
| Modèle de capacités détaillé (format, révocation, délégation) | Traité | `specs-techniques.md` §2.3 |
| Design du scheduler agent-aware (batching, préemption) | Traité | `specs-techniques.md` §3.6, §3.6.1, §3.5.5 |
| Architecture du filesystem sémantique | Traité (v1) | `specs-techniques.md` §6 |
| Format du manifeste double (agents + UI) | Traité | `specs-techniques.md` §7 |
| Modèle économique / marketplace de modules | **Toujours ouvert** | registre local seulement en v1 ; distribution/monétisation hors périmètre (`specs-fonctionnelles.md` §2.2) |

### Nouvelles pistes identifiées lors de la revue de complétude

- Support multi-utilisateur (comptes séparés) — actuellement hors périmètre v1, mono-utilisateur + agents multiples (`specs-fonctionnelles.md` §2.3).
- Mécanisme de mise à jour de l'OS lui-même (image A/B, rollback) — désormais esquissé (`specs-techniques.md` §10.1), à approfondir en implémentation.
- Score de confiance / autonomie graduée — désormais détaillé (`specs-techniques.md` §4.7, Trust Manager), reste à valider empiriquement (quels facteurs pèsent le plus).
- Gestion énergie/thermique du Placement Manager sur matériel mobile (aarch64 laptop/edge) — non traité, non prioritaire tant que la cible reste desktop/serveur.
- Contrôle d'egress réseau pour les modules génériques (au-delà des backends modèles) — désormais traité (`specs-techniques.md` §9.5).
