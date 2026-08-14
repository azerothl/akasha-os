# akasha-os

**Agent OS** — système d'exploitation agent-natif (capacités, IPC sémantique,
GPU first-class, offline-first). Voir `reflexion-agent-os.md`,
`specs-fonctionnelles.md`, `specs-techniques.md` et
`plan-developpement-phases.md`.

## État d'avancement : P0 ✅ / P1 ✅ / P2 ✅ / P3 ✅ / P4 ✅ / PV.1–PV.3 ✅ / P5.1 ✅ / PC 🚧

### P0 — Simulateur (validé)

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P0.1 | `crates/aos-placement` | Simulateur du Placement Manager (§3.5) |
| P0.2 | `crates/aos-caps` | Modèle de capacités logique (§2.3), 20 tests de sécurité |
| P0.3 | `crates/aos-registry` | Catalogue YAML (`data/models/catalog.yaml`) + backends simulés |
| P0.4 | `crates/aos-sim` | 6 scénarios §17.2 + validation croisée llama.cpp (`xval`) |

### P1 — Model Subsystem réel (gate 6/6)

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P1.1/P1.2/P1.3 | `crates/aos-llama`, `crates/aos-model` | Backend llama.cpp FFI (CUDA), placement réel, scheduler + cancellation, métriques |
| P1.4 | `crates/aos-agent` | Agent Runtime : workers = processus isolés, caps logiques, état cognitif |
| P1.5 | `crates/aos-ipc` | Semantic IPC Bus v1 : broker, CBOR, intents typés, streams |
| P1.6 | `crates/aos-ui` | TUI : shell conversationnel + dashboard ressources + contrôle agents |

### P2 — Modules WASM + mémoire + audit/undo (gate 6/6)

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P2.1/P2.2 | `crates/aos-platform` (`module_rt`) | Runtime wasmtime sandboxé (sans WASI), injection de caps, introspection, registry |
| P2.3 | `crates/aos-platform` (`memory`) | Working + episodic (embeddings llama.cpp réels) + partagée, API `mem.*` |
| P2.4 | `crates/aos-platform` (`storage`) | FS versionné COW logique, transactions, undo, classification §6.4 |
| P2.5 | `crates/aos-platform` (`audit`) | Journal append-only chaîné (hash + HMAC-SHA256), `audit.query/verify` |
| P2.6 | `modules/notes` + `modules/sdk` | Module « notes » double-surface, convention `TOOL:` côté agents |

Mesures du gate P1 (RTX 4080S) : TTFT warm **18 ms**, 32B Q6 en offload
6,12 GiB VRAM + 19,9 GiB RAM à **2 tok/s** (simulateur P0 : −5 % d'erreur).

### P3 — Backends distants + sécurité complète (gate 4/4)

| Livrable | Emplacement | Contenu |
|----------|-------------|---------|
| P3.1 | `crates/aos-model` (`backend.rs`) | Backend remote OpenAI-compatible (reqwest + SSE), clé via service secrets |
| P3.2 | `crates/aos-platform` (`policy.rs`) + routage modeld | Policy engine déclaratif (3 effets), `secret` → jamais remote |
| P3.3 | `crates/aos-platform` (`net.rs`) | Egress deny-by-default, caps `net.connect`, offline strict, journal |
| P3.4 | `crates/aos-platform` (`confirm.rs`) | Confirmation bloquante fail-closed (timeout → refus audité) |
| P3.5 | `crates/aos-platform` (`trust.rs`) | Trust Manager : score, paliers low/medium/high, `cap.request` |
| P3.6 | `crates/aos-platform` (`supervisor.rs`) | Notifications dédupliquées + arbitrage conflits de transactions |

Gate P3 vérifié contre un **mock SSE local** (`aos-gate-p3`) : secret routé
local (0 hit distant), `local_only` sans egress, `fs.delete` confirmé puis
refusé au timeout, trust élevé/faible.

### P4 — Caps natives + isolation (gate 4/4)

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P4.1 | `adr/0001-microkernel.md` | Noyau de caps userspace sur l'hôte ; port seL4 reporté (GPU) |
| P4.2 | `crates/aos-capkd` | Point d'application unique : mint/derive/grant/revoke/check, révocation globale immédiate |
| P4.3 | `crates/aos-ipc` | Caps natives `cap://kernel/<id>` dans l'enveloppe du bus |
| P4.4 | `crates/aos-auditd` + daemons | Audit autonome ; Model/Agent/Storage/Policy/CapKernel = processus séparés |
| P4.5 | `crates/aos-ui`, `aos-ui-egui` | Shells sur l'hôte (compositor microkernel = P5) |
| P4.6 | `demo/run-demo.ps1 -Gate p4` | Boot offline → assistant |

Gate P4 : services isolés (lookup), révocation kernel **cross-process**
(platformd refuse dès le revoke), kill d'`aos-auditd` sans casser
l'inférence, assistant local.

### PV — Piste VM seL4 (échafaudage noyau)

| Livrable | Emplacement | Contenu |
|----------|-------------|---------|
| PV.1 | `vm/sel4/` | Boot Microkit QEMU `virt` aarch64, PDs capkd/auditd/gate |
| PV.2 | `vm/sel4/bus.c` | Bus d'intents `cap.*` (PPC seL4, pas TCP) |
| PV.3 | `aos-caps`, `aos-sel4-capkd` | CapStore `no_std` lié dans le PD capkd (staticlib aarch64) |

```powershell
.\demo\run-sel4-vm.ps1
```

### P5.1 — Continuous batching (gate hôte)

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P5.1 | `aos-llama`, `aos-model` | `generate_batch` (prefill packé, KV unifié), dispatcher `n_seq_max=8` |
| P5.2–P5.5 | — | Écarts : 1 GPU, AccelDevice fer nu, UI/aarch64 reportés |

Gate P5.1 (RTX 4080 SUPER) : 8 flux **8/8 en 168 ms (×0,77 vs unitaire
216 ms)**. Multi-GPU non bloquant sur cet hôte.

### PC — Preview cohorte (hôte installable)

| Livrable | Emplacement | Contenu |
|----------|-------------|---------|
| PC.1 | `crates/aos-session` | Superviseur de session (boot daemons + egui) |
| PC.2 | `packaging/` | Archives Win/Linux + install scripts |
| PC.3 | `aos-ui-egui` | UI testeur (onboarding, notes, feedback…) |
| PC.4 | `feedback.submit` | Retours locaux `var/feedback/` |

```powershell
# Dev
$env:AOS_HOME = "E:\akasha-os"
cargo run -p aos-session --release

# Paquet distribuable
.\packaging\build-preview.ps1
```

Voir `INSTALL.md` et `docs/TESTER.md`.

## Licence

**Double licence.**

- **AGPL-3.0-only** (`LICENSE`) — usage, modification et distribution libres,
  y compris commerciaux, tant que le copyleft AGPL est respecté (source
  disponible, y compris pour un service réseau). Conserver `NOTICE` et citer
  [Akasha OS](https://github.com/azerothl/akasha-os).
- **Licence commerciale** (`LICENSE-COMMERCIAL.md`) — pour un fork ou un
  produit **propriétaire** (code fermé, SaaS sans publication du source).
  Attribution obligatoire ; redevance sur le CA du produit commercialisé.
  Contact : loic.peaudecerf@fasst.io.

Les marques **Akasha OS** et **Agent OS** sont réservées. Un fork doit
prendre un autre nom de produit. Contributions : `CONTRIBUTING.md`.

## Utilisation

```powershell
# Tous les tests
cargo test --workspace

# Banc d'essai P0 (6 scénarios §17.2)
cargo run -p aos-sim

# Validation croisée du modèle de coût vs llama.cpp
cargo run -p aos-sim --example xval

# Démo P1 : services + gate P1
.\demo\run-demo.ps1

# Démo P2 : build module notes + services + gate P2
.\demo\run-demo.ps1 -Gate p2

# Démo P3 : services (+ timeout confirmation 3 s) + gate P3
.\demo\run-demo.ps1 -Gate p3

# Démo P4 : caps natives + isolation de services + gate P4
.\demo\run-demo.ps1 -Gate p4

# Démo P5 : continuous batching (8 flux) + gate P5
.\demo\run-demo.ps1 -Gate p5

# Piste VM seL4 (phase PV) — ADR 0001
.\demo\run-sel4-vm.ps1

# Preview cohorte (phase PC)
$env:AOS_HOME = (Resolve-Path .)
cargo run -p aos-session --release

# Démo + UI TUI (shell conversationnel + dashboard)
.\demo\run-demo.ps1 -Gate p2 -Ui

# Arrêter les services
.\demo\run-demo.ps1 -Stop
```

Commandes UI : texte libre (chat, avec connaissance système d'Agent OS),
`/commands` (liste complète), `/help` (état du système : services, agents,
mémoire, modèles, audit), `/agent <tâche>`, `/kill <id>`, `/pause`,
`/resume`, `/steer <id> <txt>`, `/load <modèle> [profil]`, `/models`,
`/modules`, `/install <dir>`, `/notes`, `/note <titre>`,
`/notenew <titre> | <contenu>`, `/notesearch <requête>`, `/audit [n]`,
`/undo <chemin>`, `/confirm <id>`, `/deny <id>`, `/trust <agent> [score]`,
`/routing balanced|local_only|remote_only`, `/quit`. Navigation :
**PageUp/PageDown** pour scroller la conversation (suivi auto en bas).

## Documentation

- `adr/0001-microkernel.md` — P4 : noyau de caps userspace ; piste VM seL4
  (`vm/sel4/`, `.\demo\run-sel4-vm.ps1`)
- `adr/0002-model-placement.md` — algorithme de placement, modèle de coût,
  étalonnage (incl. point hybride GPU+CPU à −5 % en P1)
- `adr/0003-ui-framework.md` — **choix UI accepté : egui/eframe 0.31** après
  prototypes comparatifs (`crates/aos-ui-egui`, `crates/aos-ui-iced`) ;
  iced conservé en recours, tauri exclu du shell v1 (webview vs microkernel P4)
- `adr/0005-offload-etat-de-l-art.md` — état de l'art offload (Accelerate,
  DeepSpeed, AirLLM, FlexGen, PowerInfer, ORT GenAI) et décisions P1

## Conventions

- Rust stable, workspace cargo ; build llama.cpp via `llama-cpp-sys-2`
  (cmake + MSVC + CUDA requis pour `aos-llama`).
- `.cargo/config.toml` : incrémental désactivé (verrous Windows).
- `tools/` (binaires llama.cpp, GGUF) non versionné ; `target/demo-logs/`
  contient les logs des services de démo.
