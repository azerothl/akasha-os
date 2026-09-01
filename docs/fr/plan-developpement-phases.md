# Plan de développement par phase — Agent OS

**Langue :** [English](../development-plan.md) | Français

> Version : 1.5  
> Date : 19/08/2026  
> Statut : plan de référence  
> Références : `specs-fonctionnelles.md`, `specs-techniques.md`, `reflexion-agent-os.md`, `FEATURES.md`, `plan-evolutions.md`

---

## 0. Vue d'ensemble

Le développement d'Agent OS a **deux couches** à ne pas fusionner :

1. **Phases de fondation (P0–P5 + PV + PC)** — prouver placement, caps, WASM,
   sécurité, isolation hôte, échafaudage seL4, et la Preview installable.
   Fer nu = après PV (ADR 0001).
2. **Incréments Preview (P03–P09)** — livrer des releases de l’appli hôte
   **sans inventer une gate P6** tant que la cohorte PC est ouverte. Numérotation
   `P0n` / Preview `0.n.0` (E* du [plan-evolutions.md](plan-evolutions.md)).

P0–P5 prouvent et polissent le système **sur l'hôte** ; **PV** est l'échafaudage
**noyau seL4** (VM QEMU, sans GPU) ; **PC** est la **Preview distribuable**
pour une cohorte de testeurs (installeur Win/Linux/macOS, pas seL4). La stratégie
`specs-techniques.md` §1.3 reste : prouver les algorithmes agentiques en
userspace avant d'engager le port microkernel.

| Phase | Socle | Objectif central | Durée indicative | État |
|-------|-------|------------------|------------------|------|
| **P0** | Simulateur / proto Rust standalone | Valider l'**algorithme de placement** RAM/GPU/disque et le modèle de capacités | ~6-8 semaines | fait |
| **P1** | Linux host (processus isolés) | **Model Subsystem complet** : inférence locale, offload réel, agents multi-process, UI minimale | ~10-14 semaines | fait |
| **P2** | Linux host | **Modules WASM + mémoire + audit/undo** | ~8-10 semaines | fait |
| **P3** | Linux host | **Backends distants, routage privacy, sécurité complète** | ~6-8 semaines | fait |
| **P4** | Hôte (caps userspace) | **Sémantique microkernel** (`aos-capkd`, isolation processus) — seL4 reporté (GPU) | ~12-16 semaines | fait |
| **PV** | QEMU + seL4 (sans GPU) | **Port noyau réel** : PDs Microkit, IPC seL4, rejeu gate P4 CPU-only | ~8-10 semaines | PV.1–PV.3 ✅ |
| **PC** | Hôte Win/Linux/macOS | Preview installable : session, egui, feedback cohorte (3 Win + 1 Linux + 1 Mac) | ~2-4 semaines | 🚧 cohorte |
| **P5** | Hôte GPU / polish | **GPU/NPU first-class**, multi-GPU, polish (parallèle à PV/PC) | ~10-12 semaines | P5.1 ✅ |
| **P03** | Hôte Preview Win/Linux | Preview **0.3.0** — E1–E5 (chemin CPU, scheduler, tasks, UI caps, métriques) | livré | fait |
| **P04** | Hôte Preview Win/Linux | Preview **0.4.0** — E6 / E7-lite / E10-lite (graphe mémoire, vault, revue caps) | livré | fait |
| **P05** | Hôte Preview Win/Linux | Preview **0.5.0** — E14 (mémorisation auto depuis le chat) | livré | fait |
| **P06** | Hôte Preview Win/Linux | Preview **0.6.0** — E8 / E7-keyring / E10 (schémas pont, keyring OS, catalogue) | livré | fait |
| **P07** | Hôte Preview Win/Linux | Preview **0.7.0** — E15 (UI de module déclarative rendue par l’hôte) | livré | fait |
| **P08** | Hôte Preview Win/Linux | Preview **0.8.0** — E16 image+audio + E17 hôte CPU/GPU + désinstall module + widgets E15 + Providers | livré | fait |
| **P09** | Hôte Preview Win/Linux | Preview **0.9.0** — E18 migrate mid-token + E19 modèles/options média + plugins chat média | livré | fait |
| **P10** | Hôte Preview Win/Linux | Preview **0.10.0** — E7 TPM + E8 live + E9 + polish Media + seL4 interne | livré | fait |
| **P11** | Hôte Preview Win/Linux | Preview **0.11.0** — E20 decode local (KV Q8, prefix cache, prompt-lookup C1) | livré | fait |

**Total indicatif** (fondation) : ~60-80 semaines en séquence naïve ;
**PV ∥ P5 ∥ PC** rapproche le chemin critique. Les incréments Preview
(P03–P09) tournent sur la stack hôte déjà livrée. Ces durées sont des
ordres de grandeur — chaque gate de sortie prime sur le calendrier.

---

## Principe transverse : les gates de validation

Chaque phase se termine par un **gate de sortie** : une démonstration exécutable et un critère mesurable. **On ne démarre pas la phase suivante tant que le gate n'est pas passé.** Cela évite l'accumulation de dette technique sur les couches basses.

| Gate | Critère mesurable (gate de sortie) |
|------|-----------------------------------|
| **Gate P0** | Simulation correcte des 6 scénarios de placement (§17.2 specs-tech) avec est. tok/s cohérent avec mesures réelles sur llama.cpp |
| **Gate P1** | Boot Linux démo → assistant conversationnel (modèle embarqué) + inférence réussie sur un modèle dont la taille > VRAM (offload actif), TTFT < 2s warm |
| **Gate P2** | Installation d'un module double-surface utilisé par un agent (outil) et un humain (UI), audit trail visible, undo d'une action fichier effectif |
| **Gate P3** | Bascule automatique local→distant selon politique privacy, mode `local_only` vérifiable par egress monitoring, confirmation bloquante sur action sensible |
| **Gate P4** | Services essentiels isolés + caps natives userspace (`aos-capkd`) sur l'hôte ; kill Audit sans impacter Model |
| **Gate PV** | Boot seL4/Microkit sous QEMU `virt` (sans GPU) ; intents `cap.*` via PD bus ; révocation immédiate ; stop Audit sans tuer CapKernel |
| **Gate PC** | 3 testeurs Windows + 1 Linux + 1 macOS Apple Silicon installent Preview sans toolchain ; chemin de 15 minutes TESTER.md ; ≥1 `feedback.submit` exploitable chacun |
| **Gate P5** | Continuous batching multi-agents avec période de dégradation < 20% sur 8 flux simultanés, multi-GPU pipeline fonctionnel, port aarch64 validé sur au moins une machine cible |
| **Gate P03** | Métriques + UI caps + boot CPU-only + fire scheduler + tasks dual-surface — [phases/phase-preview-03.md](phases/phase-preview-03.md) |
| **Gate P04** | Graphe mémoire typé + vault + revue de caps module — [phases/phase-preview-04.md](phases/phase-preview-04.md) |
| **Gate P05** | Extract chat opt-in → `mem.user.remember` (secrets filtrés) — [phases/phase-preview-05.md](phases/phase-preview-05.md) |
| **Gate P06** | Schémas pont + keyring OS + catalogue local signé — [phases/phase-preview-06.md](phases/phase-preview-06.md) |
| **Gate P07** | Schéma `declarative_ui` fermé ; onglet egui générique lié aux outils du module — [phases/phase-preview-07.md](phases/phase-preview-07.md) |
| **Gate P08** | Image + TTS locaux sous caps ; artefact CPU/GPU unifié ; désinstall module ; pack widgets E15 ; onglet Providers ; install en une ligne par OS ; nettoyage/refacto — [phases/phase-preview-08.md](phases/phase-preview-08.md) |
| **Gate P09** | Migration CPU ↔ GPU en milieu de token sans cancel du stream live ; migrate en échec → fallback cancel+restart 0.8 (audité) ; schéma d’options image/TTS fermé + packs extra (Flux2, Ideogram4, voix Piper) ; page studio Image + carte d’options TTS dans le chat ; reliquat d’hygiène P08.8 — [phases/phase-preview-09.md](phases/phase-preview-09.md) |

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

- [x] Boot démo Linux → assistant conversationnel sur `embedded-instruct` (TTFT < 2s warm) — **validé sur l'hôte de dev Windows (TTFT warm mesuré : 21 ms) ; le run Linux reste à rejouer (WSL2 présent sur l'hôte, toolchain Rust à y installer)**
- [x] Inférence réussie sur un modèle de 32B Q6 avec seulement 8 Go de VRAM (offload RAM+disque actif, visible sur dashboard) — **mesuré : 6,12 GiB VRAM + 19,9 GiB RAM (11/53 couches), 1,72 tok/s ; tier DISK couvert par mmap lazy paging sur cet hôte (modèle < RAM), le streaming contraint reste à démontrer avec modèle > RAM**
- [x] Deux agents concurrents s'exécutent en parallèle sans crash mutuel — **2 processus workers isolés, production simultanée vérifiée par `aos-gate-p1`**
- [x] Kill d'un agent sans impact sur le Model Subsystem ni l'UI — **vérifié par `aos-gate-p1` (taskkill /F + inférence post-kill OK)**

> Statut P1 (12/08/2026) : gate passé sur l'hôte de dev (Windows + RTX 4080S + CUDA) avec `aos-gate-p1` — 6/6 critères exécutables verts. Écarts documentés : hôte Windows au lieu de Linux (code cross-platform, run Linux à rejouer), scheduler = files par priorité + cancellation (continuous batching mature = P5), UI = TUI ratatui (décision GUI formelle reportée, voir `adr/0003-ui-framework.md`), pause agent = abandon + régénération (reprise au token près = P5).

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

- [x] Le module « notes » est installé, utilisé par un agent (création d'une note via outil) et par un humain (UI) — **agent : convention `TOOL:` du worker → `module.invoke` ; humain : `module.invoke` en `human:ui` depuis l'UI TUI**
- [x] Audit trail montre la chaîne complète : intent → agent → outil → fs — **`tool.invoke (agent:…)` → `fs.write (module:notes)` sous un même `trace_id`, intégrité HMAC vérifiée**
- [x] Undo d'une création de fichier par agent restaure l'état antérieur — **COW logique (versions), « n'existait pas avant »**
- [x] Un module tentant d'accéder à un fichier sans capacité est refusé et audité — **agent sans `tool.invoke:notes` → refus + événement d'audit**

> Statut P2 (12/08/2026) : gate passé sur l'hôte de dev avec `aos-gate-p2` — 6/6 critères exécutables verts. Écarts documentés : index vectoriel brute-force exact (swap ANN usearch/hnswlib-rs via trait `VectorIndex`), snapshots FS = manifestes logiques userspace (pas de btrfs/ZFS sur l'hôte), paquet module = répertoire `.aospkg/` (archive signée plus tard), revue des caps à l'installation auto-approuvée en démo (UI de revue en P3).
>
> Retours utilisateur post-gate (12/08/2026), intégrés immédiatement : scroll du panneau conversation (PageUp/PageDown + suivi auto), `/commands` (liste) et `/help` (état OS : services, agents, mémoire, modèles, audit), prompt système « connaissance d'Agent OS » injecté dans l'assistant et les agents (`aos_proto::SYSTEM_ASSISTANT_PROMPT`). Reste pour P5 (P5.4 UI avancée) : recherche dans l'historique, panneau de transparence graphique, navigation clavier complète.

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

- [x] Un intent référençant une donnée `secret` est systématiquement routé local, même si un backend distant est configuré — **vérifié par `aos-gate-p3` : 0 hit sur le mock SSE, `policy.deny (deny_remote_secret)` audité, réponse servie par le modèle local**
- [x] Mode `local_only` : aucun paquet sortant vers les backends modèles détecté (vérifié par monitoring egress) — **journal `net.egress_log` vide pour le backend + 0 hit mock ; tout l'egress transite par le Backend Manager (point de contrôle unique userspace)**
- [x] Une action `fs.delete` déclenche une confirmation bloquante ; timeout → refus audité — **confirmation bloquante 3 s (config gate), refus fail-closed audité (`confirmation.resolved approved=false`), fichier intact**
- [x] Un agent avec score de confiance élevé obtient une capacité supplémentaire sans confirmation ; un agent avec score faible est refusé — **Trust Manager à paliers : high → `Granted` immédiat, low → `Denied`**

> Statut P3 (12/08/2026) : gate passé sur l'hôte de dev avec `aos-gate-p3` — 4/4 critères exécutables verts. Le backend distant est testé contre un **mock SSE OpenAI-compatible local** (pas de clé API réelle requise) ; le client reqwest/SSE est complet. Superviseur v1 minimal (notifications dédupliquées + arbitrage de conflits de transactions FS). Écarts : chiffrement des secrets = fichier local en v1 (enveloppe hardware/TPM reportée), revue des caps à l'installation toujours auto-approuvée en démo.

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

- [x] Tous les services essentiels (Model, Agent, Storage, Policy, Audit) tournent comme processus isolés, plus le noyau de caps (`aos-capkd`) — **vérifié par `aos-gate-p4` via `bus.lookup`**
- [x] Kill d'un service non critique (Audit) sans impact sur Model Subsystem ni UI — **`aos-auditd` tué ; `model.list` + inférence OK**
- [x] Une capability révoquée au niveau kernel est immédiatement invalide pour tous les processus — **mint → `fs.write`/`fs.read` via platformd → revoke → `cap.check` et `fs.read` refusés sans délai**
- [x] Boot offline → assistant conversationnel fonctionnel (même niveau que Gate P1) — **inférence locale sans réseau**

> Statut P4 (12/08/2026) : gate passé sur l'hôte de dev avec `aos-gate-p4` — 4/4 critères exécutables verts. **Décision ADR 0001** : noyau de caps userspace (`aos-capkd`) + isolation processus sur l'hôte ; le port seL4/Redox (drivers GPU) est reporté. IPC sémantique = même bus, caps natives `cap://kernel/<id>` dans l'enveloppe. UI = TUI/egui sur l'hôte (compositor microkernel = P5). Caps d'agents workers encore logiques (P1) ; l'accès fs est jugé par le noyau dès qu'une cap kernel est présentée. Enveloppe hardware des secrets (TPM) reportée.

### Risques spécifiques

| Risque | Mitigation |
|--------|-----------|
| Drivers GPU/NPU sur microkernel trop complexes | Utiliser un hyperviseur léger ou passerelle virtio (device passthrough) en P4, driver natif en P5 |
| Complexité seL4 (vérification formelle = courbe d'apprentissage) | Formation équipe + commencer par les services les moins critiques (Audit) pour monter en compétence |

> **Décision structurante (P4, ADR 0001)** : le bring-up seL4 + GPU est trop coûteux sur l'hôte Windows **pour P4**, pas comme abandon de la cible. P4 v1 = noyau de caps userspace (`aos-capkd`) sur l'hôte. **Cible produit : machine qui boot Agent OS (seL4), sans autre OS.** Chemin : hôte (GPU) et VM QEMU seL4 sans GPU en parallèle, puis fer nu (`AccelDevice` natif, P5.3). Pas de passthrough GPU depuis Windows.

> **Piste VM** : extraite en **phase PV** (ci-dessous) — n'est plus un écart de P4, c'est le port noyau.

---

## Phase PV — Piste VM seL4 (échafaudage noyau)

### Objectif

Porter la sémantique validée en P4 (caps, isolation, IPC) sur un **vrai
seL4**, dans une VM QEMU **sans GPU**. **Sortie : gate P4 rejoué dans
l'invité**, contrat de transport = primitives seL4. Le fer nu réutilise
cette image (ADR 0001). **Parallèle à P5** (GPU sur l'hôte).

### Livrables

| # | Livrable | Description |
|---|----------|-------------|
| PV.1 | Boot Microkit | Image `qemu_virt_aarch64`, PDs `capkd` / `bus` / `auditd` / `gate` |
| PV.2 | Bus sémantique | PD `bus` : lookup + proxy `cap.*` (PPC seL4, pas TCP) |
| PV.3 | CapStore `no_std` | `aos-caps` sans `std` ; staticlib `aos-sel4-capkd` liée dans le PD capkd |
| PV.4 | Préparation fer nu | Doc boot (même image, pas de virtio-CUDA) ; `AccelDevice` reste P5.3 |

### Dépendances techniques

- SDK seL4 Microkit (prébuilt), QEMU `system-aarch64`, WSL Ubuntu sur l'hôte Windows
- `libmicrokit` (glue C des PDs) ; `CapStore` Rust `no_std` dans capkd (PV.3)

### Gates de sortie (Gate PV)

- [x] Boot seL4 sous QEMU + révocation immédiate + stop Audit sans tuer CapKernel — **`AOS_GATE_VM_PASS` (PV.1, 12/08/2026)**
- [x] Les intents `cap.*` transitent par un PD bus (pas d'appel direct gate→capkd) — **lookup + proxy PPC, serial `bus lookup cap.* OK`**
- [x] `aos-caps` `no_std` : 100 % des tests de sécurité P0.2 — **20/20 ; `cargo check -p aos-caps --no-default-features`**
- [x] Contrat ABI C/Rust unique (`vm/sel4/abi.h` ≡ `aos-sel4-abi`) — **test `aligne_sur_abi_h`**
- [x] `CapStore` exécuté dans l'invité (plus de table C dupliquée) — **staticlib `aos-sel4-capkd`, gate VM rejoué**

```powershell
.\demo\run-sel4-vm.ps1
```

> Statut PV (12/08/2026) : **PV.1–PV.3 passés** (boot + bus d'intents +
> `CapStore` dans l'invité). PV.4 (fer nu) reporté. Voir `phase-vm-sel4.md`.

### Risques spécifiques

| Risque | Mitigation |
|--------|-----------|
| Toolchain seL4 absente de Windows natif | Build/run dans WSL Ubuntu ; SDK Microkit gitignoré |
| Port Rust PD bloqué (`sel4-microkit`) | CapStore lié en staticlib ; glue C tant que le runtime Rust n'est pas calé |

---

## Phase PC — Preview cohorte (hôte distribuable)

### Objectif

Livrer **Agent OS Preview 0.1** : même stack hôte (P1–P5) **installable**
par des testeurs externes sur Windows et Linux x64 (NVIDIA recommandé ;
chemin CPU OK) ou macOS Apple Silicon, **sans** compiler. UI egui =
surface principale ; retours via `feedback.submit` (local only). **Ce
n'est pas le fer nu** (ADR 0001).

### Livrables

| # | Livrable | Description |
|---|----------|-------------|
| PC.1 | `aos-session` | Superviseur : AOS_HOME, configs, boot ordonné, watchdog auditd, UI |
| PC.2 | Paquet Preview | `bin/` + download GGUF au 1er run + notes.aospkg ; install Win/Linux |
| PC.3 | UI egui cohorte | Onboarding, notes, confirm, agents, audit, scénarios, bannière |
| PC.4 | Feedback | Intent `feedback.submit` → `var/feedback/` + issue GitHub optionnelle |
| PC.5 | Docs | `docs/INSTALL.md`, `docs/TESTER.md`, `docs/FEATURES.md`, scripts `packaging/` |
| PC.6–PC.9 | Sessions / mémoire / search / fichiers | Chat persisté, `mem.context`, net opt-in, generate |
| PC.10 | Updates Releases | Overlay non destructif de `bin/` + `share/` |
| PC.11 | Transparence | Timeline agent, sources, pause / steer / retry |
| PC.12 | Settings | Préférences persistées (langue, routage, trust, moteur) |
| PC.13 | Browse + moteurs | `web.browse` ; Brave / DuckDuckGo / Bing |
| PC.14 | Bootstrap agent | `task.assess` + mémoire d'abord ; strip think Qwen |

### Gates de sortie (Gate PC)

- [x] Installeur / archive Win + Linux + macOS Apple Silicon sans `cargo`
- [x] Chemin de 15 minutes `docs/TESTER.md` jouable depuis egui
- [ ] Gate cohorte : 3 Windows + 1 Linux + 1 macOS Apple Silicon ; ≥1
      `feedback.submit` exploitable chacun

```powershell
.\packaging\build-preview.ps1
# Linux : ./packaging/build-preview.sh
```

> Statut PC (18/08/2026) : Preview **0.8.0** est la release hôte courante
> (image/TTS, artefact CPU/GPU unifié, Providers). PC.1–PC.14 livrés depuis 0.1 ; le travail
> produit suivant est les incréments P03–P09, pas un nouveau numéro PC.n.
> Gate cohorte encore ouverte — voir `docs/INSTALL.md` et `docs/FEATURES.md`.

### Risques spécifiques

| Risque | Mitigation |
|--------|-----------|
| Taille GGUF (~2–3 Go) | Embarquer uniquement 3B+embed ; 32B hors paquet |
| CUDA / drivers hétérogènes | Prérequis `nvidia-smi` ; pas de fallback CPU en 0.1 |
| Confusion « OS installé » | Bannière Preview explicite dans l'UI |

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
| P5.4 | UI avancée | Accessibilité (F-UI-08) ; la Preview livre déjà le panneau de transparence egui + control bar (PC.11) |
| P5.5 | Port aarch64 validé | Exécution stable sur au moins une machine ARM64 cible |
| P5.6 | Stabilisation & release | Corrections, documentation, critères d'acceptation globaux (specs-fonctionnelles §9) |

> Statut P5 (15/08/2026) : **P5.1 passé** sur l'hôte de dev avec `aos-gate-p5` — 8 flux 8/8 en ×0,77 wall vs unitaire (NFR-04). Écarts : P5.2 multi-GPU non testable (1× RTX 4080 SUPER), P5.3 AccelDevice = fer nu (ADR 0001), P5.5 aarch64 reporté. P5.4 transparence + control bar livrés dans egui Preview (PC.11) ; accessibilité restante. Dispatcher : fenêtre de rassemblement + `generate_batch` (prefill packé, KV unifié). Chemin unitaire reste `generate()` (P1).

### Gates de sortie (Gate P5)

- [x] 8 flux d'inférence simultanés avec dégradation < 20% vs unitaire (NFR-04) — **`aos-gate-p5` : 8/8, wall ×0,77 vs unitaire (216 ms → 168 ms)**
- [ ] Multi-GPU : un modèle réparti sur 2 GPU avec pipeline fonctionnel — **écart : 1 GPU physique sur l'hôte de dev**
- [ ] Tous les critères d'acceptation globaux de `specs-fonctionnelles.md` §9 cochés
- [ ] Port aarch64 validé sur machine cible

---

## Incréments Preview (P03–P09)

Ces phases s’empilent **sur** la stack hôte PC. Elles **ne remplacent pas**
P0–P5 / PV / PC et **ne sont pas** une gate P6. Détail :
`docs/fr/phases/phase-preview-0n.md`. Catalogue livré : [FEATURES.md](FEATURES.md).
Priorités : E1–E19 dans [plan-evolutions.md](plan-evolutions.md).

### P03 — Preview 0.3.0 (E1–E5) — fait

Prouver la thèse OS dans l’UI testeur et élargir la cohorte.

| # | Évolution | Livrable |
|---|-----------|----------|
| P03.1 | E5 | TTFT / tok/s / VRAM live dans la barre + Models |
| P03.2 | E4 | Onglet Caps : `cap.list` + revoke (audité) |
| P03.3 | E1 | Boot CPU-only + pack first-run `cpu` + packaging CPU |
| P03.4 | E2 | `schedule.*` cap-gated + UI Settings |
| P03.5 | E3 | Module `tasks` dual-surface + onglet Tasks |

### P04 — Preview 0.4.0 (E6 / E7-lite / E10-lite) — fait

| # | Évolution | Livrable |
|---|-----------|----------|
| P04.1–P04.2 | E6 | Graphe mémoire typé + UI Mémoire + bootstrap structuré |
| P04.3 | E7-lite | `vault.enc` chiffré ; secrets Settings ; MCP `${secret:…}` |
| P04.4 | E10-lite | Revue de caps à `module.install` ; exemple MCP |
| P04.5 | Docs | Contrat sibling-bridge ; quatre artefacts CUDA/CPU |

### P05 — Preview 0.5.0 (E14) — fait

`mem.extract` post-tour opt-in → `mem.user.remember` + E6
`updates`/`supersedes`. Filtre secrets. Toggle Settings (défaut on).
`user.ask` en cours de tâche.

### P06 — Preview 0.6.0 (E8 / E7-keyring / E10) — fait

| # | Évolution | Livrable |
|---|-----------|----------|
| P06.1–P06.2 | E8 | Export JSON Schema `mem.*` / `secrets.*` ; contrat HTTP JSON ↔ CBOR (pas de daemon live) |
| P06.3 | E7 | Clé maître du vault dans le keyring OS (CredMan / Secret Service ; fallback fichier 0600) |
| P06.4 | E10 | `share/modules/catalogue.yaml` local signé ; vérif hash à l’install |
| P06.5 | Stretch | Stop chat → `model.cancel` ; copie presse-papiers |

### P07 — Preview 0.7.0 (E15) — fait

**Arbre de widgets fermé** (`declarative_ui`) rendu par l’hôte egui. Un
module installé obtient un onglet générique sans webview et sans onglet
shell codé à la main. Vocabulaire : `column`, `row`, `heading`, `text`,
`markdown`, `stat_row`, `table`, `line_chart`, `form`, `button`.
Notes/Tasks restent codés à la main.

### P08 — Preview 0.8.0 (E16 + E17 + widgets E15 + Providers) — fait

**Objectif :** **génération d’image** et **génération audio (TTS)** locales
comme charges du Model Subsystem, un **artefact hôte CPU/GPU unifié**
(bascule UI + auto selon la charge), **désinstallation complète d’un
module** depuis l’UI (F-MOD-01), un **élargissement du vocabulaire E15**,
un onglet **Providers** (F-MDL-04),
**une commande d’install par OS**, plus une
**passe de nettoyage / refacto** avant le tag. Pas un sidecar cloud-only.
Pas de vidéo. Pas de voix always-on (STT / voix 24/7 au sibling). Pas de
webview.

Pourquoi ça appartient à l’OS : diffusion et TTS **se disputent la VRAM**
avec le LLM chargé (éviction F-PLC-06 / refus+alternative F-PLC-09). Un
agent n’a pas le droit ambiant de générer du média ; il lui faut
`media.generate` + `fs.write` sur l’arbre des téléchargements. La politique
device est le même Placement Manager, pas un second installeur. Un auteur
de module ne peut toujours pas inventer un `kind` ; 0.8 élargit la liste
**côté hôte** (`form` typé, select/radio, `bar_chart`, `image`/`audio`).
P3 parle déjà OpenAI-compat ; 0.8 expose des **providers nommés** (cloud +
loopback) dans l’UI au lieu d’une seule clé Settings.

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P08.1 | E16 registre | Image + TTS dans le catalogue / offerings ; shards évincables vs le LLM | fait |
| P08.2 | E16 image | Intent `media.image.generate` (prompt → PNG sous `/downloads`) ; revue de caps ; audit | fait |
| P08.3 | E16 audio | Intent `media.audio.generate` (texte → WAV/OGG TTS) ; même famille de caps ; audit | fait |
| P08.4 | E16 surface | Le chat montre l’image / joue le clip (kinds `image` / `audio` en P08.11) | fait |
| P08.5 | E17 device | Un artefact Win + un Linux ; Settings gpu/cpu/auto sans réinstall ; auto suit la charge VRAM/CPU (hystérésis) | fait |
| P08.6 | E16 packs | Packs média optionnels (téléchargés, pas cuits dans le zip) ; le même Download installe sd.cpp / piper dans `bin/` s’il manque ; GPU préféré pour l’image | fait |
| P08.7 | F-MOD-01 | Désinstaller tout module non bundlé depuis l’UI ; révoquer les caps ; retirer l’onglet E15 ; audit | fait |
| P08.8 | Hygiène | Nettoyage + refacto des crates hôte Preview (code mort, découpes, nommage) ; pas de changement de comportement | fait |
| P08.9 | Install CLI | Une commande documentée par OS : download + sha256 + overlay dans le préfixe stable | fait |
| P08.10 | Docs / ship | Docs de phase, FEATURES/STATUS/TESTER, version 0.8.0, site, packaging | fait |
| P08.11 | E15 widgets | Vocabulaire fermé : `form` typé + `select` / `radio` / `checkbox` / `textarea` / `bar_chart` / `image` / `audio` | fait |
| P08.12 | F-MDL-04 | Onglet Providers : cloud OpenAI-compat (OpenRouter, OpenAI, Anthropic, DeepSeek, z.ai) + local (Ollama, vLLM, LM Studio) | fait |

Séquençage : P08.1 → P08.2 ∥ P08.3 ∥ P08.5 ∥ P08.7 ∥ P08.11 ∥ P08.12 → P08.4 → P08.6 → P08.8 → P08.9 → P08.10.  
Détail : [phases/phase-preview-08.md](phases/phase-preview-08.md).

**Hors P08 :** génération vidéo, APIs image cloud comme chemin par défaut,
micro always-on / STT, canaux de messagerie. La migration de device en
milieu de token sans cancel est **P09 / E18**, pas un abandon. Familles
d’image extra et options sd.cpp / Piper sont **P09 / E19**. Aussi hors :
E7 TPM, daemon HTTP sibling live, E9 / P5.2 (il faut un 2e GPU), compositor
E13, fermeture cohort PC, macOS, fer nu, APIs natives Anthropic Messages /
Gemini / Bedrock (presets OpenAI-compat seulement).

### P09 — Preview 0.9.0 (E18 + E19) — fait

**Objectif :** garder un stream `model.infer` **live** quand on passe CPU ↔
GPU (pin UI ou `auto` charge / pression VRAM E16), et **étendre la
génération média locale**. 0.8 change déjà de device par cancel + restart
de `aos-modeld`, et fige sd.cpp `-W 512 -H 512 --steps 20` plus les défauts
Piper sur un seul pack SD 1.5 / deux voix. 0.9 migre KV/état : les tokens
déjà affichés restent ; si migrate échoue : fallback 0.8, audité. **E19**
ajoute un schéma d’options **fermé** (clés inconnues refusées — pas d’argv
brut depuis un agent), des packs image optionnels extra (familles Flux2,
Ideogram4) et des voix Piper extra, plus Settings / intents pour les
choisir. Les extras d’offering (VAE / CLIP / T5 / LoRA) vivent au
catalogue, pas en chemins saisis par l’utilisateur. Le chat gagne des
**plugins média hôte** : une page studio Image (réglages sd.cpp avancés)
et une carte d’options TTS dans le fil — pas des plugins marketplace WASM,
pas de webview.

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P09.1 | E18 migrate | Infer actif CPU ↔ GPU sans abort du stream | fait |
| P09.2 | E18 policy | UI/`auto` live utilise migrate ; cancel+restart 0.8 = fallback | fait |
| P09.3 | E19 schéma | Objets d’options fermés sur `media.*` + Settings ; flags sd.cpp / Piper allowlistés | fait |
| P09.4 | E19 catalogue | Packs image optionnels extra (Flux2, Ideogram4) + voix Piper extra ; `extra_files` ; VRAM | fait |
| P09.5 | E19 surface | Models / Settings choisissent pack + options ; `/image` et TTS les honorent | fait |
| P09.8 | E19 plugins chat | Page studio Image (style / LoRA / VAE / …) ; bouton sur une image du chat pour y basculer ; demande TTS → carte d’options dans le fil | fait |
| P09.7 | Hygiène | Finir le reliquat P08.8 : découper les modules hôte encore trop gros ; chrome bilingue restant ; clé de rôle chat | fait |
| P09.6 | Docs / ship | FEATURES/STATUS/TESTER, version 0.9.0, site, packaging | fait |

Séquençage : P09.1 → P09.2 ; P09.3 ∥ P09.4 → P09.5 ∥ P09.8 → P09.7 → P09.6. Dépend de P08 / E16+E17.  
Détail : [phases/phase-preview-09.md](phases/phase-preview-09.md).

### P10 — Preview 0.10.0 — fait

E7 TPM + E8 `aos-bridged` live + E9 chemin multi-GPU + polish Media + seL4 interne. Détail : [phases/phase-preview-10.md](phases/phase-preview-10.md).

### P11 — Preview 0.11.0 (E20 decode local) — fait

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P11.1 | E20 KV | `LoadOptions.kv_type` Q8_0 (GPU) / F16 ; KV typé Placement | fait |
| P11.2 | E20 préfixe | prefill suffixe `memory_seq_rm` + warm `llama_state_*` | fait |
| P11.3 | E20 lookup | Speculative prompt-lookup C1 ; batch N>1 inchangé | fait |
| P11.4 | E20 métriques | `draft_accept` / `prefix_hit` + UI | fait |
| P11.5 | Docs / ship | FEATURES/STATUS/TESTER, version 0.11.0 | fait |

Détail : [phases/phase-preview-11.md](phases/phase-preview-11.md).

### Reste après 0.11 (pas une nouvelle P6)

P11 est l’incrément Preview courant. Le reste est planifié quand le
hardware ou un daemon existe ; toujours Horizon B / C :

| Item | Notes |
|------|--------|
| **E7 TPM** | Livré en 0.10 (enveloppe hôte ; pas de PCR) |
| **E8 live** | Livré en 0.10 (`aos-bridged` opt-in) |
| **E9 / P5.2** | Chemin code en 0.10 ; hard-green = run 2 GPU documenté |
| **P5.3 / E11** | `AccelDevice` + fer nu (après PV.4) |
| **P5.5** | Hôte aarch64 validé |
| **P5.4 restant** | Accessibilité (F-UI-08) |
| **Cohorte PC** | 3 Windows + 1 Linux + 1 macOS Apple Silicon ; indépendante des incréments Preview |
| **E12 / E13** | Horizon C (préemption cognitive, compositor / webview optionnelle) |

---

## Matrice de dépendances entre phases

```
P0 (simulateur)
 └──> P1 (Model Subsystem réel)      [P0 valide l'algo de placement]
       └──> P2 (Modules + mémoire)    [P1 fournit l'IPC et le runtime d'agents]
       │     └──> P3 (Remote + sécu)  [P2 fournit le sandbox et l'audit]
       │           └──> P4 (caps userspace hôte) [P3 fige les interfaces]
       │                 ├──> P5 (GPU first-class, hôte) [parallèle]
       │                 ├──> PV (seL4 VM, sans GPU) [port noyau]
       │                 │     └──> fer nu (produit) [PV vert + AccelDevice P5.3]
       │                 └──> PC (Preview 0.1 installable)
       │                       └──> P03 → P04 → P05 → P06 → P07 → P08 → P09
       │                             (incréments Preview sur l'hôte ; pas P6)
       └──> (P1.7 aarch64, parallèle à P1, non bloquant)
```

**Points critiques** :
- P1 dépend de P0 (validation de l'algo avant d'écrire le vrai Placement Manager)
- P4 dépend de P3 (on ne porte pas des interfaces encore en mouvement)
- **PV et P5 sont parallèles** (ADR 0001) : GPU sur l'hôte, noyau dans la VM
- Le fer nu attend un gate PV vert, pas un passthrough GPU depuis Windows
- **P03–P09 n'attendent pas la gate cohorte PC** ; ils ne doivent pas inventer un numéro P6
- P08 (E16/E17) dépend du Model Subsystem P1 + E15 P07 si des kinds de widgets sont ajoutés ; il **ne** dépend **pas** de E9 / TPM / HTTP sibling live
- P09 (E18 + E19) dépend de P08 E16 moteurs + E17 cancel+restart (migrate et packs média extra sur cette base)

---

## Ressources et priorisation

### Découpage par flux de travail

| Flux | Responsabilités | Phases principales |
|------|-----------------|-------------------|
| **Flux Noyau & Sécurité** | Microkernel, caps, IPC, drivers | P0, P4, **PV**, P5 |
| **Flux Modèles & Inférence** | Model Subsystem, placement, scheduler, backends | P0, P1, P3, P5, **P08**, **P09** |
| **Flux Agents & UX** | Agent Runtime, UI, modules, mémoire, audit | P1, P2, P3, P5, **PC**, **P03–P09** |

Une équipe de 3-5 personnes peut couvrir ces 3 flux avec des rotations ; les phases sont pensées pour être majoritairement séquentielles mais avec des recouvrements partiels (ex. P1.7 aarch64 en parallèle de P1).

### Priorités par phase (rappel)

Les exigences `Must` de `specs-fonctionnelles.md` doivent être **toutes couvertes à la fin de P3** (sauf celles explicitement liées au microkernel, couvertes en P4). Les `Should` et `Could` sont répartis sur P4/PV/P5 ou reportés si nécessaire. Les incréments Preview (P03–P09) couvrent les priorités produit E* sur l'hôte sans attendre le reste P5.2 / PV.4 / cohorte PC.

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
- `FEATURES.md` — catalogue Preview livrée (actuellement 0.8.0)
- `STATUS.md` — résumé des phases livrées
- `reflexion-agent-os.md` — cadrage et pistes ouvertes
- `paysage-concurrentiel.md` — enquête OS / runtimes agentiques (août 2026)
- `plan-evolutions.md` — priorités post-paysage E1–E19 (pas une gate P6)
- `phases/phase-preview-03.md` … `phase-preview-09.md` — plans d’incréments Preview
- (ADRs publiés) : `adr/0001-microkernel.md` (P4 hôte + **phase PV** seL4 VM), `adr/0002-model-placement.md` (P0), `adr/0003-ui-framework.md` (accepté : egui), `adr/0005-offload-etat-de-l-art.md` (pré-P1)
