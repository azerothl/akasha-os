# Plan de développement par phase — Agent OS

> Version : 1.0  
> Date : 11/08/2026  
> Statut : plan de référence  
> Références : `specs-fonctionnelles.md`, `specs-techniques.md`, `reflexion-agent-os.md`

---

## 0. Vue d'ensemble

Le développement d'Agent OS est découpé en **6 phases (P0 à P5)**, chacune avec un **gate de validation** mesurable avant de passer à la suivante. La stratégie générale, définie dans `specs-techniques.md` §1.3, est de **prouver les algorithmes agentiques en userspace avant d'engager le port sur un vrai microkernel** — le risque principal étant la complexité du noyau dédié, pas la logique métier.

| Phase | Socle | Objectif central | Durée indicative |
|-------|-------|------------------|------------------|
| **P0** | Simulateur / proto Rust standalone | Valider l'**algorithme de placement** RAM/GPU/disque et le modèle de capacités | ~6-8 semaines |
| **P1** | Linux host (processus isolés) | **Model Subsystem complet** : inférence locale, offload réel, agents multi-process, UI minimale | ~10-14 semaines |
| **P2** | Linux host | **Modules WASM + mémoire + audit/undo** | ~8-10 semaines |
| **P3** | Linux host | **Backends distants, routage privacy, sécurité complète** | ~6-8 semaines |
| **P4** | seL4 / Redox (microkernel) | **Port des services sur caps natives**, IPC sémantique | ~12-16 semaines |
| **P5** | Agent OS dédié | **GPU/NPU first-class**, multi-GPU, polish | ~10-12 semaines |

**Total indicatif** : ~52-70 semaines (~1 an – 1,5 an) à taille d'équipe constante de 3-5 ingénieurs. Ces durées sont des ordres de grandeur, pas des engagements — chaque gate de sortie est plus important que le respect du calendrier.

---

## Principe transverse : les gates de validation

Chaque phase se termine par un **gate de sortie** : une démonstration exécutable et un critère mesurable. **On ne démarre pas la phase suivante tant que le gate n'est pas passé.** Cela évite l'accumulation de dette technique sur les couches basses.

| Gate | Critère mesurable (gate de sortie) |
|------|-----------------------------------|
| **Gate P0** | Simulation correcte des 6 scénarios de placement (§17.2 specs-tech) avec est. tok/s cohérent avec mesures réelles sur llama.cpp |
| **Gate P1** | Boot Linux démo → assistant conversationnel (modèle embarqué) + inférence réussie sur un modèle dont la taille > VRAM (offload actif), TTFT < 2s warm |
| **Gate P2** | Installation d'un module double-surface utilisé par un agent (outil) et un humain (UI), audit trail visible, undo d'une action fichier effectif |
| **Gate P3** | Bascule automatique local→distant selon politique privacy, mode `local_only` vérifiable par egress monitoring, confirmation bloquante sur action sensible |
| **Gate P4** | Services essentiels (Model, Agent, Storage, Policy) tournant sur microkernel avec caps natives, kill d'un service sans impact sur les autres, IPC sémantique fonctionnelle |
| **Gate P5** | Continuous batching multi-agents avec période de dégradation < 20% sur 8 flux simultanés, multi-GPU pipeline fonctionnel, port aarch64 validé sur au moins une machine cible |

---

## Phase P0 — Preuve d'algorithmes (simulateur)

### Objectif

Valider, sans écrire encore d'OS, que l'algorithme de placement RAM/GPU/disque produit des plans réalistes et que le modèle de capacités est cohérent. **Sortie : un simulateur Rust autonome.**

### Livrables

| # | Livrable | Description |
|---|----------|-------------|
| P0.1 | Simulateur de Placement Manager | Programme Rust qui prend en entrée un modèle (taille, nb couches), une config matérielle (VRAM/RAM/disque) et un profil, et qui produit un plan de placement + estimation tok/s/TTFT |
| P0.2 | Modèle de capacités logique | Types Rust `Cap`, `Rights`, opérations `mint/derive/grant/revoke` avec tests de sécurité (atténuation, révocation en cascade) |
| P0.3 | Registry + fake backends | Catalogue de modèles en YAML, backends mockés qui simulent les temps de réponse |
| P0.4 | Banc d'essai scénarios | Les 6 scénarios de `specs-techniques.md` §17.2 automatisés |

### Dépendances techniques

- Rust stable, `serde`, `criterion` (bench)
- Aucune dépendance OS spécifique (tout est standalone)

### Gates de sortie (Gate P0)

- [ ] Les 6 scénarios de placement passent avec une erreur d'estimation de tok/s < 30% vs mesures réelles sur llama.cpp (validation croisée)
- [ ] Modèle de capacités : 100% des tests de sécurité passent (atténuation stricte, révocation en arbre)
- [ ] Documentation de l'algorithme de placement publiée dans `adr/0002-model-placement.md`

### Risques spécifiques

| Risque | Mitigation |
|--------|-----------|
| L'estimation de tok/s diverge trop de la réalité | Étalonnage empirique avec llama.cpp dès P0 (mesures réelles pour caler le modèle de coût) |

---

## Phase P1 — Model Subsystem réel (userspace Linux)

### Objectif

Remplacer les mocks par une implémentation réelle : inférence locale avec llama.cpp, offload actif RAM/disque, agents multi-process avec caps logiques, UI minimale. **Sortie : un démonstrateur Linux utilisable, sans encore de vrai noyau dédié.**

### Livrables

| # | Livrable | Description |
|---|----------|-------------|
| P1.1 | Model Subsystem v1 | Registry, Backend Manager (llama.cpp via FFI), Tokenizer Service, Metrics Exporter |
| P1.2 | Placement Manager réel | Remplacement du simulateur par une implémentation qui pilote réellement les allocations VRAM/RAM/mmap |
| P1.3 | Inference Scheduler v1 | Files par priorité, batching simple, cancellation |
| P1.4 | Agent Runtime v1 | Processus isolés par agent (pas encore de caps kernel), lifecycle complet, Cognitive State sérialisable |
| P1.5 | Semantic IPC Bus v1 | Bus de messages typés (CBOR) entre processus |
| P1.6 | UI minimale | Shell conversationnel + dashboard ressources (VRAM/RAM/disque/agents) |
| P1.7 | Validation aarch64 best-effort | Compilation et exécution sur Apple Silicon ou ARM64 Linux en parallèle (non bloquant) |

### Dépendances techniques

- llama.cpp (FFI via `llama-cpp-sys` ou équivalent)
- `tokio` (async), `serde`, `ciborium` (CBOR)
- UI : choix à figer en P1 (egui/iced/tauri, voir décision §14 specs-tech)

### Gates de sortie (Gate P1)

- [ ] Boot démo Linux → assistant conversationnel sur `embedded-instruct` (TTFT < 2s warm)
- [ ] Inférence réussie sur un modèle de 32B Q6 avec seulement 8 Go de VRAM (offload RAM+disque actif, visible sur dashboard)
- [ ] Deux agents concurrents s'exécutent en parallèle sans crash mutuel
- [ ] Kill d'un agent sans impact sur le Model Subsystem ni l'UI

### Risques spécifiques

| Risque | Mitigation |
|--------|-----------|
| Perf de l'offload disque trop basse pour être utilisable | Profil `memory-saver` agressif + prefetch DMA + NVMe recommandé (cf. specs-tech §18) ; validation croisée avec le simulateur P0 pour détecter tôt |
| Choix de framework UI figeant mal | Prototype egui ET iced sur une semaine, décision ADR `0003` avant de continuer |

---

## Phase P2 — Modules WASM + mémoire + audit/undo

### Objectif

Rendre le système **extensible** et **auditable** : sandbox WASM, mémoire épisodique, audit trail, undo filesystem. **Sortie : le système devient une plateforme sur laquelle on peut installer des modules.**

### Livrables

| # | Livrable | Description |
|---|----------|-------------|
| P2.1 | Module Registry + Module Runtime | Format `.aospkg`, manifeste double-surface, chargement sandboxé WASM (wasmtime) |
| P2.2 | Caps logiques v2 | Injection de capacités dans les modules WASM, introspection de schéma |
| P2.3 | Memory Subsystem | Working memory par agent + store vectoriel (hnswlib-rs) + API `mem.*` |
| P2.4 | Storage Subsystem v1 | FS avec snapshots (btrfs/ZFS ou fallback copy-on-write logique), transactions, undo |
| P2.5 | Audit trail | Journal append-only signé, consultable via UI |
| P2.6 | Module de référence | Un module « notes » complet démontrant la double surface (outil agent + UI humaine) |

### Dépendances techniques

- `wasmtime` (runtime WASM)
- `hnswlib-rs` ou `usearch` (index vectoriel)
- Filesystem : btrfs/ZFS si dispo, sinon implémentation snapshot logique en userspace

### Gates de sortie (Gate P2)

- [ ] Le module « notes » est installé, utilisé par un agent (création d'une note via outil) et par un humain (UI)
- [ ] Audit trail montre la chaîne complète : intent → agent → outil → fs
- [ ] Undo d'une création de fichier par agent restaure l'état antérieur
- [ ] Un module tentant d'accéder à un fichier sans capacité est refusé et audité

### Risques spécifiques

| Risque | Mitigation |
|--------|-----------|
| Performance du sandbox WASM insuffisante pour outils lourds | Prévoir un mode « privilégié natif » (hors WASM) pour les modules système critiques seulement, avec revue de code stricte |

---

## Phase P3 — Backends distants + sécurité complète

### Objectif

Ajouter les **backends distants**, le **routage privacy-aware**, la **sécurité complète** (egress, confirmation bloquante). **Sortie : le système est complet fonctionnellement en v1 userspace.**

### Livrables

| # | Livrable | Description |
|---|----------|-------------|
| P3.1 | Backend Remote OpenAI-compatible | Client HTTP/SSE, auth via caps secrets |
| P3.2 | Routage local/distant | Policy engine appliquant `local_only`/`remote_only`/`balanced`, classification de sensibilité (F-FS-05) |
| P3.3 | Network Egress Control | Capacités `net.connect`, deny-by-default, mode offline strict |
| P3.4 | Confirmation bloquante | Effet `require_confirmation` dans le Policy Engine, flux IPC `pending_confirmation`, UI Control bar |
| P3.5 | Trust Manager v1 | Score de confiance, paliers, gouvernance utilisateur |
| P3.6 | Agent superviseur v1 | Agrégation notifications + arbitrage de conflits de ressources |

### Dépendances techniques

- Client HTTP : `reqwest` (Rust)
- Moteur de policy : langage déclaratif simple (YAML → moteur de règles Rust)

### Gates de sortie (Gate P3)

- [ ] Un intent référençant une donnée `secret` est systématiquement routé local, même si un backend distant est configuré
- [ ] Mode `local_only` : aucun paquet sortant vers les backends modèles détecté (vérifié par monitoring egress)
- [ ] Une action `fs.delete` déclenche une confirmation bloquante ; timeout → refus audité
- [ ] Un agent avec score de confiance élevé obtient une capacité supplémentaire sans confirmation ; un agent avec score faible est refusé

### Risques spécifiques

| Risque | Mitigation |
|--------|-----------|
| Complexité du moteur de policy | Commencer avec 3 effets seulement (`allow`/`deny`/`require_confirmation`), syntaxe volontairement limitée |

---

## Phase P4 — Port sur microkernel (seL4 / Redox)

### Objectif

C'est le **point de bascule** : porter les services userspace validés sur un vrai microkernel capability-based. **Sortie : Agent OS ne dépend plus de Linux.**

### Livrables

| # | Livrable | Description |
|---|----------|-------------|
| P4.1 | Choix du microkernel + bring-up | seL4 (recommandé pour vérification formelle) ou Redox ; boot minimal, init, drivers de base |
| P4.2 | Caps natives | Remplacement des caps logiques P1-P3 par des capabilities kernel |
| P4.3 | IPC sémantique native | Port du Semantic IPC Bus sur les primitives IPC du microkernel |
| P4.4 | Port des services | Model Subsystem, Agent Runtime, Storage, Policy, Audit — chacun comme processus serveur |
| P4.5 | UI sur microkernel | Compositor minimal + port des shells (peut être partiel au début) |
| P4.6 | Boot offline complet | Séquence de boot §10 sur microkernel, modèles embarqués chargés |

### Dépendances techniques

- seL4 (avec `sel4-sys` / bindings Rust) ou Redox OS
- Drivers : GPU/NPU et NVMe en userspace microkernel (c'est le plus gros risque)

### Gates de sortie (Gate P4)

- [ ] Tous les services essentiels (Model, Agent, Storage, Policy, Audit) tournent sur le microkernel
- [ ] Kill d'un service non critique (ex. Audit) sans impact sur Model Subsystem ni UI
- [ ] Une capability révoquée au niveau kernel est immédiatement invalide pour tous les processus
- [ ] Boot offline → assistant conversationnel fonctionnel (même niveau que Gate P1)

### Risques spécifiques

| Risque | Mitigation |
|--------|-----------|
| Drivers GPU/NPU sur microkernel trop complexes | Utiliser un hyperviseur léger ou passerelle virtio (device passthrough) en P4, driver natif en P5 |
| Complexité seL4 (vérification formelle = courbe d'apprentissage) | Formation équipe + commencer par les services les moins critiques (Audit) pour monter en compétence |

> **Décision structurante** : si le Gate P4 s'avère trop coûteux (drivers GPU), il est possible de rester sur Linux en production v1 tout en gardant l'architecture capability-based logique — voir ADR `0001-microkernel.md` (à rédiger au début de P4).

---

## Phase P5 — GPU first-class + polish

### Objectif

Exploiter pleinement le GPU/NPU comme citoyen de première classe du scheduler, multi-GPU, et polish général. **Sortie : Agent OS v1.0.**

### Livrables

| # | Livrable | Description |
|---|----------|-------------|
| P5.1 | Continuous batching mature | vLLM-like, intégration profonde avec le scheduler natif |
| P5.2 | Multi-GPU pipeline | Répartition de couches inter-GPU (pipeline parallelism) |
| P5.3 | AccelDevice natif | Remplacement de la passerelle virtio par un trait natif si P4 l'exigeait |
| P5.4 | UI avancée | Transparency panel complet, Control bar avancée, accessibilité (F-UI-08) |
| P5.5 | Port aarch64 validé | Exécution stable sur au moins une machine ARM64 cible |
| P5.6 | Stabilisation & release | Corrections, documentation, critères d'acceptation globaux (specs-fonctionnelles §9) |

### Gates de sortie (Gate P5)

- [ ] 8 flux d'inférence simultanés avec dégradation < 20% vs unitaire (NFR-04)
- [ ] Multi-GPU : un modèle réparti sur 2 GPU avec pipeline fonctionnel
- [ ] Tous les critères d'acceptation globaux de `specs-fonctionnelles.md` §9 cochés
- [ ] Port aarch64 validé sur machine cible

---

## Matrice de dépendances entre phases

```
P0 (simulateur)
 └──> P1 (Model Subsystem réel)      [P0 valide l'algo de placement]
       └──> P2 (Modules + mémoire)    [P1 fournit l'IPC et le runtime d'agents]
       │     └──> P3 (Remote + sécu)  [P2 fournit le sandbox et l'audit]
       │           └──> P4 (microkernel) [P3 fige les interfaces, P4 les porte]
       │                 └──> P5 (GPU first-class) [P4 fournit le socle noyau]
       └──> (P1.7 aarch64, parallèle à P1, non bloquant)
```

**Points critiques** :
- P1 dépend de P0 (validation de l'algo avant d'écrire le vrai Placement Manager)
- P4 dépend de P3 (on ne porte pas des interfaces encore en mouvement)
- P4.1 (drivers) est le goulot d'étranglement de tout P4 — à prototyper dès la fin de P3 si les ressources le permettent

---

## Ressources et priorisation

### Découpage par flux de travail

| Flux | Responsabilités | Phases principales |
|------|-----------------|-------------------|
| **Flux Noyau & Sécurité** | Microkernel, caps, IPC, drivers | P0, P4, P5 |
| **Flux Modèles & Inférence** | Model Subsystem, placement, scheduler, backends | P0, P1, P3, P5 |
| **Flux Agents & UX** | Agent Runtime, UI, modules, mémoire, audit | P1, P2, P3, P5 |

Une équipe de 3-5 personnes peut couvrir ces 3 flux avec des rotations ; les phases sont pensées pour être majoritairement séquentielles mais avec des recouvrements partiels (ex. P1.7 aarch64 en parallèle de P1).

### Priorités par phase (rappel)

Les exigences `Must` de `specs-fonctionnelles.md` doivent être **toutes couvertes à la fin de P3** (sauf celles explicitement liées au microkernel, couvertes en P4). Les `Should` et `Could` sont répartis sur P4/P5 ou reportés si nécessaire.

---

## Suivi et indicateurs

- **Revue de gate** : démonstration en conditions réelles à chaque fin de phase, pas de présentation slideware
- **Métriques suivies en continu** : TTFT embedded, tok/s sous offload, latence IPC, taux de couverture de tests
- **ADR** : chaque décision structurante (choix UI, microkernel, format weights) documentée dans `adr/` avant implémentation
- **Mise à jour des specs** : tout écart découvert en développement est reversé dans `specs-fonctionnelles.md` ou `specs-techniques.md` avec bump de version

---

## Documents liés

- `specs-fonctionnelles.md` — exigences produit
- `specs-techniques.md` — architecture technique
- `reflexion-agent-os.md` — cadrage et pistes ouvertes
- (à créer au fil de l'eau) `adr/0001-microkernel.md`, `adr/0002-model-placement.md` (publié, P0), `adr/0003-ui-framework.md`, `adr/0005-offload-etat-de-l-art.md` (publié, pré-P1)
