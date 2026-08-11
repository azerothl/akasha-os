# akasha-os

**Agent OS** — système d'exploitation agent-natif (capacités, IPC sémantique,
GPU first-class, offline-first). Voir `reflexion-agent-os.md`,
`specs-fonctionnelles.md`, `specs-techniques.md` et
`plan-developpement-phases.md`.

## État d'avancement : Phase P0 (simulateur)

La phase P0 valide les algorithmes agentiques en userspace avant tout port
microkernel. Livrables en place :

| Livrable | Crate | Contenu |
|----------|-------|---------|
| P0.1 | `crates/aos-placement` | Simulateur du Placement Manager (§3.5) : placement par profils, modèle de coût tok/s/TTFT, éviction, pression, re-profilage à chaud |
| P0.2 | `crates/aos-caps` | Modèle de capacités logique (§2.3) : `mint/derive/grant/revoke/revoke_tree`, atténuation stricte, TTL, révocation en arbre |
| P0.3 | `crates/aos-registry` | Catalogue de modèles YAML (`data/models/catalog.yaml`) + backends simulés (local/distant) |
| P0.4 | `crates/aos-sim` | Banc d'essai des 6 scénarios §17.2 + validation croisée llama.cpp |

## Utilisation

```powershell
# Tous les tests (44)
cargo test --workspace

# Banc d'essai des 6 scénarios de placement (§17.2)
cargo run -p aos-sim

# Validation croisée du modèle de coût vs mesures llama.cpp
cargo run -p aos-sim --example xval

# Sonde de bande passante RAM de l'hôte (étalonnage)
cargo run -p aos-placement --example host_probe --release

# Bench criterion de l'algorithme de placement
cargo bench -p aos-placement --bench placement
```

## Gate P0 — statut

- [x] 6 scénarios §17.2 passent (`cargo test -p aos-sim`)
- [x] Erreur tok/s < 30 % vs llama.cpp après étalonnage (mesures réelles
  b10361, hôte Ryzen 7 9800X3D : −19,9 % / +0,0 % — voir `adr/0002-model-placement.md` §4)
- [x] Modèle de capacités : 100 % des tests de sécurité passent (19 tests :
  atténuation stricte, révocation en arbre, TTL, grant)
- [x] ADR publié : `adr/0002-model-placement.md`

Limites documentées dans l'ADR : `eff_gpu` non étalonné (hôte P0 sans GPU),
streaming disque à valider sur offload réel en P1.

## Conventions

- Rust stable, workspace cargo ; dépendances P0 : `serde`, `serde_yaml`,
  `bitflags`, `thiserror`, `criterion` (bench).
- Aucune dépendance OS spécifique en P0 (standalone, Windows/Linux/macOS).
- `tools/` (binaires llama.cpp, GGUF de mesure) n'est pas versionné.
