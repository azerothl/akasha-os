# akasha-os

**Agent OS** — système d'exploitation agent-natif (capacités, IPC sémantique,
GPU first-class, offline-first). Voir `reflexion-agent-os.md`,
`specs-fonctionnelles.md`, `specs-techniques.md` et
`plan-developpement-phases.md`.

## État d'avancement : P0 ✅ / P1 ✅ (gate passé sur l'hôte de dev)

### P0 — Simulateur (validé)

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P0.1 | `crates/aos-placement` | Simulateur du Placement Manager (§3.5) |
| P0.2 | `crates/aos-caps` | Modèle de capacités logique (§2.3), 19 tests de sécurité |
| P0.3 | `crates/aos-registry` | Catalogue YAML (`data/models/catalog.yaml`) + backends simulés |
| P0.4 | `crates/aos-sim` | 6 scénarios §17.2 + validation croisée llama.cpp (`xval`) |

### P1 — Model Subsystem réel (gate 6/6 sur hôte Windows + RTX 4080S)

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P1.1/P1.2/P1.3 | `crates/aos-llama`, `crates/aos-model` | Backend llama.cpp FFI (CUDA), placement réel (plan → `n_gpu_layers`/mmap), scheduler files par priorité + cancellation, métriques |
| P1.4 | `crates/aos-agent` | Agent Runtime : workers = processus isolés, caps logiques, état cognitif sérialisable |
| P1.5 | `crates/aos-ipc` | Semantic IPC Bus v1 : broker (`aos-busd`), CBOR, intents typés, streams, découverte |
| P1.6 | `crates/aos-ui` | TUI : shell conversationnel + dashboard ressources + contrôle agents |
| Gate | `crates/aos-agent` (`aos-gate-p1`) | Vérification exécutable des 6 critères |

Mesures du gate (12/08/2026, hôte Ryzen 9800X3D + RTX 4080S 16 Go) :

- TTFT warm `embedded-instruct` : **21 ms** (cible < 2 s) ;
- 32B Q6_K, budget VRAM 8 GiB : **6,12 GiB VRAM + 19,9 GiB RAM (11/53 couches)**,
  inférence OK à **1,72 tok/s** (estimation simulateur : 1,64 tok/s → **−5 %**) ;
- 2 agents concurrents OK ; kill d'un agent sans impact (Model Subsystem + UI).

Écarts documentés (voir plan, section P1) : run Linux à rejouer (code
cross-platform, testé sur Windows), continuous batching = P5, UI produit =
décision reportée (`adr/0003-ui-framework.md`), pause agent = abandon +
régénération.

## Utilisation

```powershell
# Tests (P0 : 44 tests ; IPC : 5 tests d'intégration)
cargo test --workspace

# Banc d'essai P0 (6 scénarios §17.2)
cargo run -p aos-sim

# Validation croisée du modèle de coût vs llama.cpp
cargo run -p aos-sim --example xval

# Démo P1 complète : services + gate exécutable
.\demo\run-demo.ps1

# Démo + UI TUI (shell conversationnel + dashboard)
.\demo\run-demo.ps1 -Ui

# Arrêter les services
.\demo\run-demo.ps1 -Stop
```

Commandes UI : texte libre (chat), `/agent <tâche>`, `/kill <id>`,
`/pause`, `/resume`, `/steer <id> <txt>`, `/load <modèle> [profil]`,
`/models`, `/quit`.

## Documentation

- `adr/0002-model-placement.md` — algorithme de placement, modèle de coût,
  étalonnage (incl. point hybride GPU+CPU à −5 % en P1)
- `adr/0003-ui-framework.md` — décision UI (provisoire : TUI P1, GUI en P2)
- `adr/0005-offload-etat-de-l-art.md` — état de l'art offload (Accelerate,
  DeepSpeed, AirLLM, FlexGen, PowerInfer, ORT GenAI) et décisions P1

## Conventions

- Rust stable, workspace cargo ; build llama.cpp via `llama-cpp-sys-2`
  (cmake + MSVC + CUDA requis pour `aos-llama`).
- `.cargo/config.toml` : incrémental désactivé (verrous Windows).
- `tools/` (binaires llama.cpp, GGUF) non versionné ; `target/demo-logs/`
  contient les logs des services de démo.
