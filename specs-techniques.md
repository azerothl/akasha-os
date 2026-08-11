# Spécifications techniques — Agent OS

> Version : 0.2  
> Date : 11/08/2026  
> Statut : brouillon  
> Référence : `specs-fonctionnelles.md`, `reflexion-agent-os.md`  
> Changements v0.2 : revue de complétude — ajout Agent superviseur/System Assistant/Trust Manager (§4.5-4.7), classification de sensibilité (§6.4), Module Registry (§7), egress réseau (§9.5), confirmation bloquante (§9.4), mises à jour système (§10.1), Module API/Admin API (§11.4-11.5), profil utilisateur (§12), cibles agents concurrents (§13), accessibilité (§8.3), glossaire technique et matrice de traçabilité (§21-22).

---

## 1. Vue d'ensemble architecture

### 1.1 Principes directeurs

1. **Microkernel capability-based** pour le noyau de confiance minimale  
2. **Services système en espace utilisateur** (drivers, filesystem, model runtime, agent runtime)  
3. **IPC sémantique** comme bus natif (intentions typées, pas seulement bytes)  
4. **GPU/NPU first-class** dans le scheduler et le gestionnaire mémoire  
5. **Offline-first** : modèles embarqués + placement local obligatoire au boot  
6. **WASM/WASI** pour modules sandboxés  
7. **Rust** privilégié pour le TCB (Trusted Computing Base) et les services critiques  

### 1.2 Couches

```
┌─────────────────────────────────────────────────────────────┐
│  Surfaces utilisateur                                        │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │ UI Directe   │  │ UI Convers.  │  │ CLI / Debug      │  │
│  └──────┬───────┘  └──────┬───────┘  └────────┬─────────┘  │
├─────────┴─────────────────┴───────────────────┴─────────────┤
│  Agent Runtime          │  Module Runtime (WASM)            │
│  - lifecycle agents     │  - sandbox + caps                 │
│  - context switch cog.  │  - double-surface manifest        │
├─────────────────────────┴───────────────────────────────────┤
│  Semantic IPC Bus (intents, schemas, capability tokens)     │
├──────────────┬──────────────────┬───────────────────────────┤
│ Model        │ Memory           │ Storage                   │
│ Subsystem    │ Subsystem        │ Subsystem                 │
│ - registry   │ - working        │ - hierarchical FS         │
│ - backends   │ - episodic/vec   │ - semantic index          │
│ - placement  │ - shared         │ - snapshots / undo        │
│ - scheduler  │                  │                           │
├──────────────┴──────────────────┴───────────────────────────┤
│  System Services (user-space)                                │
│  net │ display │ input │ audit │ policy │ device-mgr         │
├─────────────────────────────────────────────────────────────┤
│  Microkernel                                                 │
│  caps │ IPC │ threads │ vm │ irq │ sched primitives          │
├─────────────────────────────────────────────────────────────┤
│  Hardware : CPU │ RAM │ GPU/NPU │ NVMe/SSD │ net             │
└─────────────────────────────────────────────────────────────┘
```

### 1.2bis Services complémentaires (non détaillés dans le schéma ci-dessus)

Le schéma en couches met l'accent sur le chemin de données principal (inférence). Les services suivants sont logés dans les couches « Agent Runtime » / « System Services » mais méritent d'être cités explicitement pour la complétude fonctionnelle :

| Service | Logé dans | Détaillé en |
|---------|-----------|-------------|
| **Module Registry** (catalogue/installation des modules) | System Services, à côté du Module Runtime | §7 |
| **Agent superviseur** (arbitrage multi-agents, notifications) | Agent Runtime, agent système privilégié | §4.6 |
| **System Assistant Agent** (assistant par défaut de l'onboarding/shell) | Agent Runtime, agent système privilégié | §4.5 |
| **Trust Manager** (score de confiance, autonomie graduée) | System Services | §4.7 |
| **Network Egress Control** (capacités réseau sortantes) | System Services (`net`) + Policy | §9.5 |
| **Admin Service** (politiques globales, quotas, modèles par défaut) | System Services (`policy`) | §11.5 |

### 1.3 Stratégie d'implémentation par phases

| Phase | Socle | Livrable |
|-------|-------|----------|
| **P0** | Linux host (dev) ou microVM | Services agentiques + Model Subsystem complets (userspace) |
| **P1** | Microkernel existant (seL4 / Redox / Zircon-like) | Port des services, caps natives |
| **P2** | Noyau dédié Agent OS (Rust) | Intégration profonde GPU sched + FS sémantique natif |

> La spec technique ci-dessous décrit la **cible P1/P2**. P0 valide les algorithmes (placement, batching, caps logiques) sans attendre le noyau final.

---

## 2. Microkernel

### 2.1 Responsabilités (TCB minimal)

- Gestion des **capabilities** (création, dérivation, révocation, rights bits)
- **IPC** synchrone/asynchrone entre tasks
- Threads / scheduling bas niveau (priorités, time slices)
- Mémoire virtuelle (address spaces, grant/map via caps)
- Délégation d'interruptions aux drivers user-space
- Horloge, timers

### 2.2 Ce qui n'est PAS dans le kernel

- Système de fichiers
- Stack réseau
- Drivers GPU (hors primitive d'accès mémoire/IRQ)
- Runtime modèles / agents
- UI

### 2.3 Modèle de capabilities

```text
Cap = {
  object_id,          // référent
  rights,             // bitmask : READ|WRITE|EXECUTE|GRANT|REVOKE|...
  badge,              // contexte opaque pour le détenteur
  ttl_optional,       // expiration
  attenuation_rules   // droits maximaux dérivables
}
```

Opérations primitives :
- `mint` / `derive` (atténuation des rights)
- `grant` (transfert à un autre address space)
- `revoke` / `revoke_tree`
- `invoke` (appel sur l'objet référencé)

Toute ressource (fichier, socket, modèle, mémoire partagée, device) est un **objet référencé par cap**.

### 2.4 IPC sémantique

Au-dessus des messages kernel bruts, le **Semantic IPC Bus** impose :

- messages **typés** (schéma versionné, ex. JSON Schema / CBOR Schema / protobuf-like)
- attachement de **caps** dans les messages
- corrélation request/response + streams
- découverte de services (`lookup("model.infer")`)

Exemple d'intent :

```json
{
  "intent": "model.infer",
  "version": 1,
  "payload": {
    "model_ref": "local:embedded-instruct-v1",
    "messages": [{"role": "user", "content": "Résume note.md"}],
    "params": {"max_tokens": 512, "temperature": 0.2}
  },
  "caps": ["cap://fs/read/notes/note.md", "cap://model/use/embedded-instruct-v1"]
}
```

---

## 3. Model Subsystem (cœur critique)

Le Model Subsystem est un **service système user-space** privilégié, découpé en composants.

### 3.1 Composants

```
Model Subsystem
├── Model Registry          # catalogue local + distant, métadonnées
├── Backend Manager         # local runtime(s) + remote clients
├── Placement Manager       # dispatch couches RAM/GPU/Disque
├── Inference Scheduler     # files, batching, priorités agents
├── Weight Store            # stockage weights, mmap, cache couches
├── Tokenizer Service       # tokenizers partagés
└── Metrics Exporter        # TTFT, tok/s, VRAM, hit rates
```

### 3.2 Model Registry

Métadonnées d'un modèle :

```yaml
id: local:llama-q6-32b
name: Llama 32B Q6
modality: text
format: gguf          # gguf | safetensors | onnx | remote-api
source:
  type: local_file
  path: /models/llama-32b-q6.gguf
  sha256: ...
architecture:
  n_layers: 80
  n_params: 32e9
  context_length: 131072
resource_hints:
  weights_bytes: 24000000000
  min_vram_full: 22000000000
  min_ram_full: 26000000000
  supports_layer_offload: true
  supports_quantizations: [q4_k, q5_k, q6_k, q8_0]
capabilities:
  - chat
  - tools
backends_compatible: [llamacpp, candle, triton]
privacy_class: local
```

Pour un modèle distant :

```yaml
id: remote:openai:gpt-4.1
source:
  type: remote_api
  endpoint: https://api.openai.com/v1
  protocol: openai_compatible
auth_cap: cap://secrets/openai_key
privacy_class: remote
```

### 3.3 Backends

| Backend | Rôle | Notes |
|---------|------|-------|
| **Embedded Runtime** | Inférence locale native (cible : llama.cpp-like / candle / ggml) | Obligatoire au boot |
| **Advanced Local** | Backend optionnel haute perf (type vLLM/TGI si portés) | Phase ultérieure |
| **Remote OpenAI-compatible** | HTTP/S SSE ou streaming custom | Cloud ou serveur privé |
| **Remote gRPC** | Option pour clusters internes | Could |

**API unifiée interne** (tous backends) :

```text
infer(request) -> stream<TokenEvent>
embed(request) -> Vector
list_models() -> []
health() -> Status
cancel(inference_id)
```

### 3.4 Modèles embarqués (bootstrap)

Livrés dans l'image système sous `/system/models/` (chemin logique) :

| Rôle | Taille cible | Usage |
|------|--------------|-------|
| `embedded-instruct` | 1B–3B quantifié (Q4/Q5) | Assistant shell, onboarding, agents légers |
| `embedded-embed` | ~100–500MB | Mémoire sémantique, FS sémantique |
| (optionnel) `embedded-router` | très petit | Classification d'intent / routage local vs remote |

Contraintes :
- chargement **garanti** en RAM (+ GPU si dispo) au premier boot
- pas de dépendance réseau
- signature / hash vérifiés au démarrage

### 3.5 Placement Manager — dispatch RAM / GPU / Disque

#### 3.5.1 Objectif

Exécuter des modèles dont la taille **dépasse la VRAM**, voire la RAM, en répartissant les **couches (layers)** et buffers associés sur trois tiers :

| Tier | Médium | Latence d'accès | Usage typique |
|------|--------|-----------------|---------------|
| T0 | VRAM GPU/NPU | très faible | couches actives hot, KV cache prioritaire |
| T1 | RAM système | faible | couches warm, KV overflow |
| T2 | Disque (SSD NVMe) | moyenne | couches cold, weights mmap |

#### 3.5.2 Unités de placement

- **Layer shard** : une couche transformer (ou groupe de couches)
- **KV cache blocks** : paginés (block size configurable, ex. 16–64 tokens)
- **Embedding / output tables** : placées selon hotness

Chaque unité a un descripteur :

```text
Shard {
  model_id,
  shard_id,
  kind: Layer | KVBlock | Embed | Other,
  size_bytes,
  residency: VRAM | RAM | DISK,
  pin_count,
  last_use_ts,
  priority_boost
}
```

#### 3.5.3 Algorithme de placement initial

Entrées :
- `W` = taille totale weights
- `V_free`, `R_free`, `D_free` = budgets libres VRAM/RAM/Disque (après réserves OS)
- profil : `latency` | `balanced` | `memory-saver` | `cpu-only`
- contraintes agent (deadline, priorité)

Pseudo-code :

```text
function place_model(model, profile, budgets):
  reserve_os_budgets()  # ne jamais tout manger

  if profile == cpu-only or no_gpu:
    assign all layers -> RAM, overflow -> DISK (mmap)
    kv -> RAM
    return plan

  # Score chaque layer par "hotness attendu"
  # (entrée/sortie souvent plus hot; milieu selon modèle)
  layers = score_layers(model)

  sort layers by score desc

  for layer in layers:
    if fits(layer, VRAM) and profile in [latency, balanced]:
      assign layer -> VRAM
    else if fits(layer, RAM):
      assign layer -> RAM
    else:
      assign layer -> DISK

  # KV cache: préférer VRAM puis RAM; jamais DISK sauf mode extrême
  place_kv_policy(profile)

  validate_plan()  # estimer tok/s min; refuser si sous seuil critique
  return plan
```

#### 3.5.4 Exécution avec offload (runtime)

Pendant le forward pass :

1. Pour chaque layer `L` dans l'ordre du graphe :
   - si `L` en VRAM → compute direct
   - si `L` en RAM → upload async vers VRAM (ou compute CPU si backend le permet)
   - si `L` en DISK → page-in vers RAM (mmap + fault) puis upload si GPU
2. **Prefetch** : anticiper layers `L+1..L+k` sur stream DMA dédié
3. **Evict** : layers les moins récemment utilisées / score bas si pression mémoire
4. Double-buffering : calculer `L` pendant transfert de `L+1`

#### 3.5.5 Pression mémoire et préemption

Triggers :
- nouvel agent haute priorité demande VRAM
- UI système signale frame drop / low memory
- seuil `vram_watermark_high`

Actions (dans l'ordre) :
1. Réduire batch size / concurrents d'inférence low-priority  
2. Migrer layers low-priority VRAM → RAM  
3. Migrer layers RAM → DISK  
4. Suspendre inférences low-priority (sauvegarde état partiel si possible)  
5. Refuser nouvelles inférences non critiques avec erreur explicite + alternatives  

#### 3.5.6 Profils de placement

| Profil | VRAM | RAM | Disque | Objectif |
|--------|------|-----|--------|----------|
| `latency` | max layers + KV | overflow | minimal | max tok/s |
| `balanced` | layers hot + KV | layers warm | cold | défaut |
| `memory-saver` | KV minimal / micro-batch | peu | majorité weights | cohabitation multi-modèles |
| `cpu-only` | 0 | max | overflow | pas de GPU |

#### 3.5.7 Multi-GPU (phase ultérieure)

- partition pipeline (layers 0..k sur GPU0, k+1..n sur GPU1)  
- ou tensor parallel si backend le supporte  
- Placement Manager expose un plan multi-device unifié  

#### 3.5.8 Weight Store

- Fichiers weights en **lecture seule**, adressables par offset
- **mmap** pour DISK→RAM zero-copy quand possible
- Cache de pages avec politique **LFU + semantic pin** (layers épinglées pour modèle système)
- Intégrité : hash par shard, vérification lazy ou au load
- Quantization runtime optionnelle (downgrade Q6→Q4 sous pression) — Could

### 3.6 Inference Scheduler

Caractéristiques :
- files par **priorité** (system critical > interactive UI > agent high > agent normal > batch)
- **batching** continu des requêtes compatibles (même modèle, mêmes dims) — inspiration vLLM continuous batching
- préemption coopérative aux frontières de tokens / layers
- fair-share entre agents d'égale priorité (weights configurables)
- cancellation immédiate propagée au backend

Structure :

```text
InferenceScheduler {
  queues: Map<Priority, Queue<InferJob>>
  running: Set<InferJob>
  model_locks / resource_tokens
  batch_window_us: 50..2000
}
```

#### 3.6.1 Quotas d'accès (F-MDL-10)

Chaque agent (et chaque module qui invoque un modèle en son nom) dispose d'un budget renouvelable de type **token bucket** :

```text
AgentQuota {
  agent_id,
  tokens_per_minute: u32,
  concurrent_inferences_max: u8,
  gpu_time_ms_per_minute: u32,
  burst_allowance: f32       // marge temporaire au-delà du quota nominal
}
```

- Dépassement de quota → job rétrogradé en file `batch` (pas de refus brutal), sauf priorité `system critical`.
- Quotas par défaut fixés par l'Admin Service (§11.5) ; ajustables par agent via policy.
- Consommation exposée dans les métriques (§13) pour transparence utilisateur.

### 3.7 Routage local vs distant

```text
function select_backend(req, policy):
  if policy.mode == local_only: return best_local(req)
  if policy.mode == remote_only: return best_remote(req)

  candidates = []
  if local_available(req.model_or_task): candidates += score_local()
  if remote_allowed(req) and network_ok(): candidates += score_remote()

  score = f(latency_est, cost, privacy_risk, load, quality_hint)
  return argmax(candidates) or fail_with_alternatives()
```

> `quality_hint` : signal de complexité de la tâche fourni par l'agent appelant (ex. `simple` / `reasoning` / `long_context`) ou dérivé par le `embedded-router` (§3.4) ; influence la préférence pour un modèle plus capable même si plus coûteux/latent.

Règles privacy :
- classification données (`public`, `private`, `secret`)
- `secret` → jamais remote
- `private` → remote seulement si policy explicite

---

## 4. Agent Runtime

### 4.1 Modèle d'exécution

- Chaque agent = **task group** : threads logiques + address space logique + cap set + cognitive state
- Exécution **asynchrone** (green threads / async runtime) pour ne pas bloquer sur I/O modèle
- Isolation : pas de mémoire partagée implicite entre agents

### 4.2 Cognitive State

```text
CognitiveState {
  agent_id,
  working_memory,      // fenêtre contexte courante
  plan_stack,          // sous-buts
  tool_session,        // appels outils en cours
  cap_set_snapshot,
  inference_handles,
  episodic_cursors,    // pointeurs mémoire long terme
  version
}
```

Opérations : `suspend`, `resume`, `snapshot`, `restore`, `migrate` (futur).

### 4.3 Lifecycle API

```text
agent.create(spec, initial_caps) -> agent_id
agent.start(agent_id)
agent.steer(agent_id, directive)
agent.pause(agent_id)
agent.resume(agent_id)
agent.kill(agent_id)
agent.grant(agent_id, cap)
agent.revoke(agent_id, cap)
```

### 4.4 Outils et modules

Un agent invoque un outil via Semantic IPC :

```text
invoke_tool(tool_id, args, caps) -> result
```

Le Module Runtime vérifie :
1. le schéma des args  
2. que l'agent détient les caps exigées par le manifeste  
3. les quotas  

### 4.5 System Assistant Agent

Agent système par défaut, seul agent démarré automatiquement à chaque boot (§10) :

- utilise `embedded-instruct` par défaut (bascule vers un modèle plus capable si disponible et autorisé)
- porte l'onboarding, le shell conversationnel, l'aide contextuelle
- **n'a pas de capacités élevées implicites** : malgré son statut « système », il obéit au même modèle de capacités que tout agent (§2.3) ; ses seules particularités sont (a) démarrage garanti, (b) accès en lecture aux métriques système non sensibles pour répondre aux questions de l'utilisateur
- toutes ses actions sont auditées comme n'importe quel agent (§9.3)

### 4.6 Agent superviseur (arbitrage & notifications)

Agent système privilégié répondant à F-UI-07 et F-AGT-10 :

```text
SupervisorAgent {
  notification_queue: PriorityQueue<Notification>,
  conflict_log: []
}
```

Responsabilités :
- **Agrégation et priorisation des notifications** (dédoublonnage, regroupement par contexte, silence des notifications non critiques selon politique utilisateur)
- **Arbitrage de conflits** entre agents concurrents : demandes de ressources incompatibles (ex. deux agents demandant la même exclusivité fichier), priorités contradictoires → décision selon règles déclaratives (priorité déclarée, ancienneté, score de confiance) puis remontée à l'humain si non résolu automatiquement
- Accès **introspectif uniquement** aux métadonnées des files (priorité, agent_id, ressource demandée) — **pas d'accès au contenu/contexte cognitif** des agents arbitrés (respect de F-SEC-06)
- Expose ses décisions via l'audit trail et le panneau de transparence UI (§8.1)

### 4.7 Trust Manager — autonomie graduée (F-AGT-09)

Service qui calcule et fait évoluer un **score de confiance** par agent (ou par classe d'agent/module) :

```text
TrustProfile {
  agent_id | agent_class,
  score: f32,                 // 0.0 – 1.0
  history_window,             // fenêtre glissante (ex. 30 jours / N actions)
  factors: {
    success_rate,             // tâches complétées sans erreur
    override_rate,            // fréquence d'annulation/correction humaine
    confirmation_denials,     // demandes de confirmation refusées par l'utilisateur
    age_days
  },
  last_updated
}
```

Fonctionnement :
- Le score est recalculé après chaque action significative (audit event) selon une fonction pondérée des facteurs ci-dessus.
- Le score **ne grant jamais automatiquement** une capacité sensible seul : il définit des **paliers** (ex. `low` / `medium` / `high`) qui déterminent si une demande de capacité peut être :
  - accordée automatiquement (palier suffisant + capacité non critique),
  - soumise à confirmation humaine (§9.4),
  - refusée d'office (palier insuffisant pour une capacité critique, quel que soit le score).
- L'utilisateur peut à tout moment consulter, geler ou réinitialiser le score d'un agent (transparence, pas de « boîte noire »).
- Le Trust Manager est un service **consultatif** : le Policy Engine (§9.4) reste l'autorité finale de décision.

---

## 5. Memory Subsystem

### 5.1 Niveaux

| Niveau | Implémentation | Durée |
|--------|----------------|-------|
| Working | buffer structuré en RAM par agent | session / tâche |
| Episodic | store vectoriel + métadonnées | long terme |
| Shared | segments partagés + caps | scope collaboration |
| System | politiques, audit indexes | permanent |

### 5.2 Store vectoriel natif

- embeddings via `embedded-embed` par défaut
- index ANN (HNSW ou équivalent)
- éviction : score = f(pertinence sémantique, recency, pin utilisateur)
- chiffrement au repos optionnel pour mémoires sensibles

### 5.3 API

```text
mem.working_get/set
mem.episodic_write(event)
mem.episodic_query(query, k, filters)
mem.shared_open(cap) / read / write
mem.export / wipe (user-facing)
```

---

## 6. Storage Subsystem

### 6.1 Hierarchical FS

- FS moderne journalisé avec **snapshots** (inspiration ZFS/btrfs/APFS)
- ACL remplacées par **caps fichiers**
- Copy-on-write pour undo agent

### 6.2 Semantic index

- service d'indexation asynchrone (texte, métadonnées, embeddings)
- requête : path classique **ou** intent (`find "contrat bail 2024"`)
- le path reste la source de vérité ; l'index est dérivé

### 6.3 Undo / transactions

```text
tx = fs.begin_transaction(agent_id)
... opérations ...
fs.commit(tx) | fs.rollback(tx)
```

Snapshots nommés avant actions à haut risque (policy).

### 6.4 Classification de sensibilité (F-FS-05)

Chaque objet du FS porte un attribut étendu (xattr-like) :

```text
DataClass = public | private | secret
```

Règles :
- **Héritage** par défaut depuis le dossier parent (`/home/<user>/secrets/**` → `secret` par défaut, `/home/<user>/documents/**` → `private` par défaut).
- **Override manuel** possible par l'utilisateur (propriétés du fichier, UI directe).
- **Classification assistée** (Should, non bloquant) : un classifieur léger (heuristiques + `embedded-embed`) peut suggérer une classe à la création d'un fichier (ex. détection de motifs type IBAN, clé API) ; l'utilisateur valide ou corrige.
- Consommée directement par le Policy Engine (§9.4) et le routage modèle (§3.7, `privacy_risk`) : un `intent` qui référence un objet `secret` ne peut pas être servi par un backend `remote`, quelle que soit la politique par ailleurs.
- Modifiable uniquement via une capacité `fs.reclassify` distincte de `fs.write` (évite qu'un agent baisse discrètement la sensibilité d'une donnée pour contourner une politique).

---

## 7. Module system (WASM)

### 7.1 Composants

Le système de modules est découpé en deux services distincts, à l'image du Model Subsystem (§3.1) :

```
Module System
├── Module Registry   # catalogue local : liste, versions, état d'installation, dépendances
└── Module Runtime    # exécution sandboxée WASM, résolution de caps au runtime
```

- **Module Registry** : source de vérité sur ce qui est installé/disponible (F-MOD-01, F-MOD-06). Registre local en v1 (fichier signé `/system/modules/registry.yaml` + `/var/modules/`), synchronisation avec un registre distant hors périmètre v1.
- **Module Runtime** : charge le binaire WASM, applique le sandbox (§7.4), route les invocations d'outils vers le module (F-MOD-04, F-MOD-05).

### 7.2 Format package

```text
module.aospkg
├── manifest.yaml
├── module.wasm
├── ui/                 # déclaratif (ex. JSON UI / web bundle sandboxé)
├── schemas/            # tools input/output
├── assets/
└── signatures/
```

### 7.3 Manifeste double-surface

```yaml
name: notes
version: 1.2.0
hash: sha256:...
permissions:
  required_caps:
    - fs.read:/documents/notes/**
    - fs.write:/documents/notes/**
tools:
  - name: notes.create
    description: Créer une note
    input_schema: schemas/create.json
    output_schema: schemas/create_out.json
  - name: notes.search
    ...
ui:
  entry: ui/index.html
  mode: sandboxed_webview  # ou declarative_ui
min_os_api: 1
```

> Exemple de capacité réseau pour un module nécessitant un accès externe (ex. recherche web) : `required_caps: [net.connect:api.example.com:443]` — soumise à revue utilisateur à l'installation et au contrôle d'egress (§9.5).

### 7.4 Sandbox

- runtime WASM (WASI preview adaptée)
- **aucun** accès host hors caps injectées
- limites CPU/mem/time par invocation
- signatures vérifiées avant install/load

---

## 8. UI subsystem

### 8.1 Composants

- **Compositor** (display server minimal)
- **Shell direct** : dock/panels, file manager, settings, resource dashboard
- **Shell conversationnel** : timeline, streaming tokens, cartes d'actions
- **Transparency panel** : chain-of-custody des décisions agent
- **Control bar** : pause / stop / steer sur agent sélectionné

### 8.2 Contraintes perf

- L'UI tourne à priorité **interactive** > agents batch
- L'inférence ne doit pas bloquer le thread UI (processus séparés + IPC)
- Budget mémoire UI réservé (non évictable par Placement Manager sauf critical)

### 8.3 Accessibilité (F-UI-08)

- Compositor expose un arbre d'accessibilité natif (rôles, labels) consommable par lecteur d'écran, indépendant du rendu graphique
- Navigation clavier complète sur les deux shells (direct + conversationnel) ; pas de piège de focus
- Contraste et taille de texte configurables globalement (thème système), respectés par les modules via le mode `declarative_ui` (§7.3)
- Le mode `sandboxed_webview` (contenu HTML libre d'un module) n'est **pas garanti** accessible par le système — recommandation aux développeurs de modules plutôt que garantie plateforme

---

## 9. Sécurité

### 9.1 Trust boundaries

1. Microkernel TCB  
2. Services système signés  
3. Modules WASM  
4. Agents  
5. Backends distants (non trusted)

### 9.2 Secrets

- service `secrets` : stockage chiffré (clé hardware si dispo, sinon clé enveloppe dérivée)
- les agents reçoivent des **caps d'usage** (ex. signer une requête) pas le secret brut, sauf exception admin

### 9.3 Audit

- journal append-only signé
- événements : grant/revoke cap, infer start/end, tool invoke, fs tx, policy deny
- non accessible en écriture aux agents applicatifs

### 9.4 Politiques

Moteur de policy (langage simple déclaratif) :

```yaml
rule: deny_remote_secret_data
match:
  data_class: secret
  backend.privacy_class: remote
effect: deny
```

Effets possibles (`effect`) :

| Effet | Comportement |
|-------|--------------|
| `allow` | Action exécutée normalement |
| `deny` | Action refusée, `PermissionDenied` auditée (§16) |
| `require_confirmation` | Action **suspendue** ; prompt envoyé à la Control bar (§8.1) avec contexte (agent, action, données concernées) ; timeout configurable → refus par défaut si pas de réponse (fail-closed) |

Exemple répondant à F-SEC-07 :

```yaml
rule: confirm_sensitive_side_effect
match:
  action.kind: [fs.delete, network.send_external, payment]
effect: require_confirmation
timeout_sec: 120
default_on_timeout: deny
```

Le flux `require_confirmation` est implémenté comme une extension du protocole d'intent (§2.4) : le Semantic IPC Bus retourne un statut `pending_confirmation` avec un `confirmation_id` ; l'agent reste suspendu (`agent.pause` implicite) jusqu'à résolution.

### 9.5 Réseau et contrôle d'egress (F-SEC-08)

Toute connexion réseau sortante initiée par un agent ou un module passe par une capacité dédiée :

```text
cap://net/connect/<host-pattern>:<port-range>
```

Règles par défaut :
- **Deny by default** : un module sans capacité réseau explicite ne peut résoudre DNS ni ouvrir de socket.
- Les capacités réseau des **backends modèles distants** (§3.3) sont gérées séparément par le Model Subsystem (déjà couvertes par `privacy_class`/routing, §3.7) et ne nécessitent pas de capacité `net.connect` générique supplémentaire pour l'agent appelant — seul le Backend Manager détient la capacité réseau correspondante.
- Pour les **modules/outils génériques** (ex. recherche web, appel API tiers), la capacité `net.connect` est demandée explicitement dans le manifeste (§7.3) et review par l'utilisateur à l'installation (comme les permissions fichiers).
- Mode « offline strict » (politique globale) : coupe l'ensemble des capacités `net.connect` actives, y compris celles déjà accordées, sans désinstallation des modules (renforce F-MDL-06).
- Toute tentative de connexion refusée est auditée (`policy deny`, §9.3) avec l'hôte cible, pour diagnostic.

---

## 10. Boot sequence (technique)

```text
1. Bootloader → microkernel
2. Init process (pid1-like) démarre services essentiels :
   device-mgr, storage, audit, policy
3. Model Subsystem :
   a. scan /system/models
   b. verify signatures
   c. detect GPU/NPU
   d. place embedded-instruct + embedded-embed
   e. warm-up minimal (1 forward pass court)
4. Agent Runtime + Module Runtime
5. UI Shell + System Assistant agent
6. User session
```

Si étape 3 échoue partiellement → mode dégradé avec messages clairs ; shell direct reste dispo.

> L'étape 6 (User session) inclut l'onboarding lors du tout premier démarrage (langue, préférences, politiques de confiance, cf. F-BOOT-03) ; budget cible : NFR-07 (< 10 min offline, specs-fonctionnelles.md §8).

### 10.1 Mises à jour système (F-BOOT-06)

- Image système en **double slot A/B** : mise à jour téléchargée/appliquée sur le slot inactif, activation au reboot suivant.
- Health-check post-update (boot réussi + services essentiels up dans un délai borné) ; échec → **rollback automatique** vers le slot précédent sans intervention utilisateur.
- Distinct des snapshots FS utilisateur (§6.1) : une mise à jour système ne modifie jamais `/home/<user>` ni `/var/agents`.
- Modèles embarqués (§3.4) versionnés indépendamment de l'image système pour permettre leur mise à jour seule (taille, cadence de release différentes).

---

## 11. APIs système (extrait)

### 11.1 Model API

| Méthode | Description |
|---------|-------------|
| `model.list` | Liste modèles registry |
| `model.inspect` | Métadonnées + placement actuel |
| `model.load` | Charge avec profil de placement |
| `model.unload` | Libère ressources |
| `model.set_placement` | Plan manuel / profil |
| `model.infer` | Inférence streaming |
| `model.embed` | Embeddings |
| `model.cancel` | Annule job |
| `model.metrics` | Métriques live |

### 11.2 Agent API

| Méthode | Description |
|---------|-------------|
| `agent.create/start/pause/resume/kill` | Lifecycle |
| `agent.steer` | Directive runtime |
| `agent.grant/revoke` | Caps |
| `agent.state` | Cognitive + status |
| `agent.audit` | Historique actions |

### 11.3 FS / Mem / Mod

Voir sections dédiées ; toutes exposées via Semantic IPC et SDK (langages : Rust, C ABI, bindings futurs).

### 11.4 Module API (F-MOD-01, F-MOD-03, F-MOD-06)

| Méthode | Description |
|---------|--------------|
| `module.list` | Liste des modules installés (Module Registry) |
| `module.search` | Recherche dans le registre local (nom, capacité fournie) |
| `module.describe` | Manifeste complet + schémas d'outils (introspection, F-MOD-03) |
| `module.install` | Installe depuis un package `.aospkg`, affiche les caps demandées pour revue |
| `module.update` | Met à jour vers une nouvelle version (diff de caps affiché si changement) |
| `module.uninstall` | Retire le module, révoque ses caps actives |
| `module.quarantine` | Isole un module suite à panic répété (§16) sans le désinstaller |

### 11.5 Admin API (F-OBS-03)

| Méthode | Description |
|---------|--------------|
| `admin.policy.list/get/set` | CRUD sur les règles du Policy Engine (§9.4) |
| `admin.quota.get/set` | Consultation/ajustement des quotas par agent ou global (§3.6.1) |
| `admin.models.set_default` | Définit le(s) modèle(s) par défaut pour l'assistant système et les nouveaux agents |
| `admin.trust.get/reset` | Consultation et remise à zéro d'un score de confiance (§4.7) |
| `admin.audit.export` | Export de l'audit trail pour analyse externe (F-OBS-04) |

> En v1 (mono-utilisateur, §12), cette API n'est pas séparée par un compte distinct : elle est accessible depuis l'UI directe sous un espace « Administration système », protégé par une confirmation renforcée pour les actions à fort impact (ex. modification globale de politique).

---

## 12. Données et schémas persistés

```text
/system/
  models/           # embarqués
  modules/          # modules système
  policies/
/var/
  models/           # modèles utilisateur
  modules/          # modules installés par l'utilisateur (Module Registry, §7.1)
  agents/           # états / snapshots
  memory/           # episodic stores
  audit/
  cache/weights/    # pages offload
/home/<user>/
  profile.yaml      # préférences, politiques, réglages Trust Manager
  documents/
  ...
```

Formats :
- weights : GGUF (v1 prioritaire), SafeTensors (roadmap)
- configs : YAML/JSON signés
- audit : JSONL append-only + checksum chain

**Profil utilisateur** (`/home/<user>/profile.yaml`) :

```yaml
user_id: local-primary
locale: fr-FR
role: admin              # v1 : toujours admin, mono-utilisateur (specs-fonctionnelles.md §2.3)
policies_overrides: []   # références vers règles admin.policy (§11.5)
model_defaults:
  assistant: local:embedded-instruct
  embed: local:embedded-embed
placement_profile_default: balanced   # §3.5.6
network_mode: online     # online | offline_strict (§9.5)
```

> Note d'implémentation : la structure prévoit déjà un `user_id` et un `role` explicites afin qu'une évolution multi-utilisateur future (hors périmètre v1) n'implique pas de migration de schéma, seulement l'ajout d'entrées `/home/<user2>/` et d'une séparation stricte des capacités entre profils.

---

## 13. Métriques et SLO techniques cibles (v1)

| Métrique | Cible indicative (machine réf. à figer) |
|----------|----------------------------------------|
| Boot → assistant prêt | < 30s SSD, modèle embarqué warm |
| TTFT embedded-instruct (warm) | < 2s |
| UI input latency | < 100ms p95 |
| Dégradation sous offload disque | tok/s ≥ 25% du full-RAM sur SSD NVMe |
| Isolation | kill -9 d'un agent sans impact kernel/UI |
| Fuite VRAM | 0 après unload + GC (tolérance fragmentation documentée) |
| Agents concurrents (NFR-04) | ≥ 32 agents légers actifs (working memory + idle) sans dégradation UI |
| Flux d'inférence concurrents (NFR-04) | ≥ 8 flux simultanés via continuous batching (§3.6) sans chute > 20% du tok/s unitaire |

Machine de référence (proposition) :
- CPU 8 cores, 32 GB RAM, GPU 8 GB VRAM, SSD NVMe 512 GB

---

## 14. Stack technologique proposée

| Domaine | Choix proposé | Alternatives |
|---------|---------------|--------------|
| Langage TCB / services | **Rust** | C/C++ pour drivers isolés |
| Kernel P1 | seL4 ou Redox / custom Rust microkernel | Zircon concepts |
| Host de dev P0 | Linux + processes | QEMU microVM |
| Inférence locale | **llama.cpp** (via FFI) ou **candle** | ggml, ort |
| Quantization formats | GGUF | GPTQ/AWQ plus tard |
| WASM runtime | wasmtime ou wasmi | |
| Index vectoriel | hnswlib-rs / usearch | sqlite-vss |
| UI | à décider (egui / iced / web compositor sandbox) | |
| IPC serialization | CBOR ou capnp | JSON (debug) |
| Build | cargo workspaces + image builder | |

> Portabilité (NFR-10) : x86_64 est la cible primaire de développement (P0/P1). Un portage aarch64 best-effort (Apple Silicon en environnement de dev, cible edge/ARM serveur) est suivi en parallèle dès M1 pour éviter une dette d'abstraction matérielle tardive, sans bloquer les jalons x86_64.

---

## 15. Interfaces avec le matériel accélérateur

### 15.1 Device Manager GPU/NPU

- inventaire devices (PCI/virtio)
- budgets mémoire exposés au Placement Manager
- streams de copie async (DMA)
- recovery sur device lost (fallback CPU)

### 15.2 Abstraction `AccelDevice`

```text
trait AccelDevice {
  info() -> DeviceInfo
  alloc(size) -> MemHandle
  free(MemHandle)
  copy_host_to_device(src, dst, stream)
  copy_device_to_host(...)
  copy_device_to_device(...)
  sync(stream)
  // compute soumis via backend d'inférence, pas via ce trait générique
}
```

Backends concrets (P0/P1) : Vulkan compute / CUDA / Metal / CPU SIMD selon plateforme.

---

## 16. Gestion des erreurs (contrats)

| Situation | Comportement |
|-----------|--------------|
| OOM VRAM pendant load | replacer automatiquement ; sinon erreur `PlacementImpossible` + suggestion profil |
| Disque plein (weights cache) | purger cache cold ; sinon refuse load |
| Backend remote timeout | retry policy ; fallback local si autorisé ; sinon erreur utilisateur |
| Cap manquante | `PermissionDenied` auditée |
| Module panic | isolation ; agent reçoit erreur outil ; module peut être mis en quarantine |
| Corruption weights | hash fail → refuse load, notification |

---

## 17. Tests et validation

### 17.1 Niveaux

- unit : placement algorithm, cap attenuation, policy engine  
- integration : infer local with forced offload tiers  
- e2e : boot offline → onboarding → tâche agent → undo  
- chaos : kill backends, pressure VRAM, disk full  
- security : sandbox escape attempts, cap forgery, audit tamper  

### 17.2 Scénarios placement obligatoires

1. Modèle < VRAM → full GPU  
2. VRAM < modèle < RAM → hybrid GPU+RAM  
3. modèle > RAM → GPU+RAM+DISK streaming  
4. 2 modèles concurrents → éviction fair/priority  
5. Passage latency → memory-saver à chaud  
6. Arrivée agent haute priorité pendant infer batch  

---

## 18. Risques techniques et mitigations

| Risque | Impact | Mitigation |
|--------|--------|------------|
| Complexité microkernel from scratch | délai | P0 sur Linux ; P1 seL4/Redox |
| Perf offload disque trop basse | UX | NVMe obligatoire recommandé ; prefetch agressif ; quantize |
| Écosystème CUDA propriétaire | portabilité | abstraction AccelDevice ; Vulkan/Metal |
| Fuite de données via remote | privacy | policy engine + local_only |
| Explosion complexité scheduler | stabilité | commencer FIFO+priorités ; batching ensuite |
| Taille image modèles embarqués | distribution | quantif forte ; téléchargement optionnel modèles plus gros après boot |

---

## 19. Roadmap technique synthétique

> Vue synthétique des milestones. Pour le détail complet (livrables, gates de sortie, dépendances, risques par phase), voir **`plan-developpement-phases.md`**.

### Milestone M0 — Spec & proto algorithms
- simulateur de Placement Manager
- registry + fake backends

### Milestone M1 — P0 userspace sur Linux
- Model Subsystem réel (llama.cpp)
- offload RAM/disk fonctionnel
- agents multi-process + caps logiques
- UI minimale + dashboard ressources
- validation portage aarch64 best-effort en parallèle (Apple Silicon dev / edge ARM), cf. §14

### Milestone M2 — Modules WASM + mémoire épisodique
- package double-surface
- store vectoriel
- audit + undo FS

### Milestone M3 — Remote backends + routage privacy
- OpenAI-compatible
- policies

### Milestone M4 — Port microkernel / caps natives
- seL4 ou équivalent
- IPC sémantique natif

### Milestone M5 — GPU scheduler first-class + multi-GPU
- intégration profonde device-mgr
- continuous batching mature

---

## 20. Annexes

### A. Exemple de plan de placement (32B Q6, GPU 8GB, RAM 32GB)

```text
Model: 24 GB weights, 80 layers
Plan (balanced):
  VRAM 7.5 GB : layers 0-18 + KV cache blocks (hot)
  RAM  14 GB  : layers 19-55
  DISK  rest  : layers 56-79 (prefetch window=2)
Estimated TTFT: ...
Estimated tok/s: ...
```

### B. Exemple flux infer avec caps

```text
Agent A --(cap model.use:llama, cap fs.read:doc)--> model.infer
InferenceScheduler enqueue (prio=interactive)
PlacementManager ensure_residency(llama)
Backend.local.stream_tokens -> Agent A
Audit.append(infer_started/finished)
```

### C. Documents liés

- `specs-fonctionnelles.md`
- `reflexion-agent-os.md`
- `plan-developpement-phases.md` — plan détaillé par phase (livrables, gates, risques)
- (futur) `adr/0001-microkernel.md`
- (futur) `adr/0002-model-placement.md`
- (futur) `adr/0003-wasm-modules.md`
- (futur) `adr/0004-scope-mono-vs-multi-utilisateur.md`

---

## 21. Glossaire technique

| Terme | Définition |
|-------|------------|
| TCB | Trusted Computing Base — code dont la correction est requise pour la sécurité globale (microkernel + services signés) |
| Shard | Unité de placement (couche transformer, bloc KV cache, table d'embedding) manipulée par le Placement Manager |
| AccelDevice | Trait d'abstraction bas niveau pour un accélérateur GPU/NPU (alloc/copy/sync), indépendant du backend d'inférence |
| Continuous batching | Fusion dynamique de requêtes d'inférence compatibles en un seul batch GPU, sans attendre la fin des requêtes en cours |
| Badge | Contexte opaque attaché à une capability, interprété par le détenteur (ex. distinguer deux usages du même droit) |
| Atténuation | Réduction des droits d'une capability lors de sa dérivation (`derive`) — jamais d'élévation |
| Weight Store | Service de stockage/mmap des poids de modèles, en lecture seule, avec cache de pages |
| Semantic IPC Bus | Bus de messages typés (intents + schémas + capabilities) au-dessus de l'IPC brut du microkernel |
| Token bucket | Mécanisme de quota à renouvellement continu utilisé pour limiter la consommation d'inférence par agent (§3.6.1) |
| Palier de confiance | Niveau discret (`low`/`medium`/`high`) dérivé du score de confiance, utilisé par le Policy Engine pour moduler l'octroi de capacités |
| Slot A/B | Double image système permettant une mise à jour avec rollback automatique (§10.1) |
| Data class | Étiquette de sensibilité (`public`/`private`/`secret`) portée par un objet du FS (§6.4) |

## 22. Matrice de traçabilité — exigences fonctionnelles → composants techniques

> Objectif : vérifier qu'aucune exigence fonctionnelle Must/Should de `specs-fonctionnelles.md` ne reste sans composant technique identifié. Revue effectuée en v0.2.

| Bloc fonctionnel | IDs | Composant(s) technique(s) | Statut |
|-------------------|-----|----------------------------|--------|
| Bootstrap | F-BOOT-01 → 06 | §10 Boot sequence, §10.1 Mises à jour système, §3.4 Modèles embarqués | Couvert |
| Runtime d'agents | F-AGT-01 → 10 | §4 Agent Runtime (4.1 Modèle d'exécution, 4.2 Cognitive State, 4.3 Lifecycle, 4.5 System Assistant, 4.6 Superviseur, 4.7 Trust Manager) | Couvert |
| Gestion des modèles | F-MDL-01 → 10 | §3 Model Subsystem (3.2 Registry, 3.3 Backends, 3.6 Scheduler, 3.6.1 Quotas, 3.7 Routage) | Couvert |
| Dispatch RAM/GPU/Disque | F-PLC-01 → 10 | §3.5 Placement Manager (3.5.1 → 3.5.8) | Couvert |
| Mémoire agentique | F-MEM-01 → 05 | §5 Memory Subsystem | Couvert |
| Stockage / filesystem | F-FS-01 → 05 | §6 Storage Subsystem (6.1 Hierarchical FS, 6.2 Semantic index, 6.3 Undo, 6.4 Classification) | Couvert |
| Modules et applications | F-MOD-01 → 06 | §7 Module system (7.1 Composants/Registry, 7.2 Format, 7.3 Manifeste, 7.4 Sandbox), §11.4 Module API | Couvert |
| Interface utilisateur | F-UI-01 → 08 | §8 UI subsystem (8.1 Composants, 8.2 Contraintes perf, 8.3 Accessibilité) | Couvert |
| Sécurité / privacy / confiance | F-SEC-01 → 08 | §9 Sécurité (9.1 Trust boundaries, 9.2 Secrets, 9.3 Audit, 9.4 Politiques + confirmation, 9.5 Egress réseau) | Couvert |
| Observabilité / administration | F-OBS-01 → 04 | §9.3 Audit, §11.5 Admin API, §13 Métriques, Metrics Exporter (§3.1) | Couvert |
| NFR (perf, fiabilité, scalabilité, sécurité, privacy, UX, extensibilité, observabilité, portabilité) | NFR-01 → 10 | §13 Métriques et SLO, §14 Stack (note portabilité), §7.4 Sandbox (NFR-05), §9.4/9.5 (NFR-06) | Couvert |

### Écarts assumés (hors périmètre v1, volontairement non couverts techniquement)

| Sujet | Raison | Suivi |
|-------|--------|-------|
| Modèle économique / marketplace distribué | Hors périmètre v1 (registre local uniquement) | `reflexion-agent-os.md` §Pistes à creuser |
| Multi-utilisateur avec isolation stricte de comptes | Hors périmètre v1 (mono-utilisateur, §12) | ADR futur `0004-scope-mono-vs-multi-utilisateur.md` |
| Gestion énergie/thermique du Placement Manager (mobile/edge) | Non prioritaire tant que la cible principale est desktop/serveur | À réévaluer si portage aarch64 mobile (§14) avance |
