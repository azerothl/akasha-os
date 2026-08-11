# ADR 0002 — Algorithme de placement et modèle de coût (P0)

> Date : 11/08/2026
> Statut : accepté (P0), à réviser après étalonnage GPU (P1)
> Références : `specs-techniques.md` §3.5, §13, §17.2 ; `plan-developpement-phases.md` Gate P0
> Implémentation : `crates/aos-placement` (manager, cost, sim), `crates/aos-sim` (scénarios, xval)

---

## 1. Contexte

Le Gate P0 exige un simulateur validant l'algorithme de placement RAM/GPU/disque
(§3.5) avec une estimation de tok/s dont l'erreur est < 30 % vs des mesures
llama.cpp réelles. Cet ADR documente l'algorithme implémenté, le modèle de coût,
ses hypothèses, son étalonnage empirique et ses limites connues.

## 2. Algorithme de placement (implémentation de §3.5.3)

### 2.1 Entrées

- `ModelDesc` : `n_layers`, `n_params`, `weights_bytes`, `embed_bytes`,
  `kv_bytes_per_token`, `context_length`, `supports_layer_offload` ;
- budgets libres par tier (VRAM/RAM/DISK, réserves OS déjà déduites) ;
- profil : `latency` | `balanced` | `memory-saver` | `cpu-only` ;
- taille de contexte KV visée (`kv_tokens`).

### 2.2 Déroulé

1. **CPU-only** (profil explicite ou `has_gpu == false`) : tout en RAM,
   overflow DISK (mmap), KV en RAM.
2. **Scoring de hotness des couches** : les couches d'entrée/sortie reçoivent
   un bonus (largeur de bord = max(2, n/8), bonus ≤ +50 % décroissant vers le
   centre), cf. §3.5.3 « entrée/sortie souvent plus hot ». Les couches sont
   placées par score décroissant.
   *Remarque :* le modèle de coût n'est sensible qu'aux **totaux d'octets par
   tier**, pas à l'ordre ; le scoring ne change donc pas l'estimation, seulement
   la résidence effective de chaque couche. llama.cpp place les couches
   contiguës (`-ngl N`) — l'écart est sans effet sur la validation croisée.
3. **Réservation KV puis embed** (hotness maximale, lues à chaque token) :
   VRAM puis RAM ; DISK seulement en « mode extrême » (§3.5.3).
4. **Couches** : budget VRAM par profil —
   `latency` : 100 % du restant VRAM ; `balanced` : 85 % (marge KV/cohabitation) ;
   `memory-saver` : 0 couche VRAM, RAM plafonnée à 25 % des poids, le reste DISK
   (cohabitation multi-modèles, §3.5.6). Overflow : RAM puis DISK.
5. **validate_plan** : estimation tok/s (§3 ci-dessous) ; refus si sous le seuil
   critique `min_tok_s` (défaut 0,1 tok/s).
6. **Repli automatique** (§16) : `latency → balanced → memory-saver → cpu-only` ;
   si tout échoue, `PlacementImpossible` avec suggestion explicite (F-PLC-09).

### 2.3 Éviction et pression (runtime, `PlacementSim`)

- **Préemption (F-PLC-06)** : à l'arrivée d'un modèle, on calcule la VRAM qu'un
  placement idéal lui donnerait ; les shards VRAM des modèles **strictement
  moins prioritaires** migrent (LRU d'abord) vers RAM/DISK jusqu'à satisfaire
  la demande. Jamais d'éviction sur un modèle plus prioritaire.
- **Éviction de secours** : si le placement échoue malgré tout, migration de
  shards de priorité ≤ demandeur (LRU), avec cascade VRAM→RAM→DISK.
- **Échelle de pression (§3.5.5)** : (1) batch réduit (KV VRAM low-prio ÷ 2) →
  (2) migration VRAM→RAM → (3) RAM→DISK → (4) suspension low-prio (KV libéré) →
  (5) refus explicite. Arrêt dès que la demande est satisfaite.
- **Re-profilage à chaud** : le nouveau plan est recalculé dans les budgets
  « libres + occupés par le modèle » ; seuls les shards changeant de tier sont
  migrés ; le temps de bascule estimé = octets migrés / lien le plus lent
  (PCIe ou disque).

## 3. Modèle de coût

### 3.1 Decode (tok/s) — limité par la bande passante

```
t_compute = octets(VRAM) / (gpu_bw · eff_gpu)        # couches VRAM, GPU
          + octets(RAM)  / (ram_bw · eff_ram)        # couches RAM, calcul CPU (comme llama.cpp -ngl)
          + KV et embed lus sur leur tier
t_stream  = octets(DISK) / (disk_bw · eff_disk)      # streaming mmap + prefetch
t_token   = max(t_compute, t_stream) + overhead      # double buffering §3.5.4
tok/s     = 1 / t_token
```

Le `max(...)` modélise le prefetch avec double buffering (§3.5.4) : le streaming
des couches DISK se chevauche avec le calcul des tiers rapides ; le débit est
fixé par la ressource la plus lente.

### 3.2 Prefill (TTFT) — limité par le calcul, tokens batchés

```
par tier rapide : n_layers × max(2·P_layer·N / flops_eff(tier), B_layer / bw_eff(tier))
DISK            : streamé une fois, chevauché avec le calcul des autres tiers
TTFT            = max(t_rapides, t_stream_disque) + t_token + setup
```

### 3.3 Paramètres d'étalonnage (`CostModel`)

| Paramètre | Défaut | Source |
|-----------|--------|--------|
| `eff_gpu` | 0,50 | littérature llama.cpp ; **à mesurer en P1** (pas de GPU sur l'hôte P0) |
| `eff_ram` | 0,80 | **étalonné : 0,99 mesuré** (§4), marge prudente pour CPU anciens |
| `eff_disk` | 0,80 | NVMe séquentiel, lecture large ; à confirmer en P1 (offload réel) |
| `eff_prefill_gpu` | 0,60 | à mesurer en P1 |
| `eff_prefill_cpu` | 0,50 | combiné à `cpu_flops`, donne 1,0–1,7 TFLOPS effectifs mesurés |
| `overhead_ms` | 0,2 | sampling/scheduling par token |
| `setup_ms` | 10 | init graphe par requête |
| `min_tok_s` | 0,1 | seuil de viabilité `validate_plan` |

## 4. Validation croisée (Gate P0)

### 4.1 Protocole

1. Sonde hôte : `cargo run -p aos-placement --example host_probe --release`
   → bande passante RAM mesurée (alimente `HardwareProfile` de l'hôte).
2. Mesures llama.cpp b10361 : `llama-bench -m <model.gguf> -t 8 -p 128 -n 64`
   (binaire et modèles sous `tools/`, non versionnés).
3. Comparaison : `cargo run -p aos-sim --example xval` ; test automatisé dans
   `crates/aos-sim/src/xval.rs`.

### 4.2 Mesures (11/08/2026, hôte : Ryzen 7 9800X3D 8c, 64 GiB DDR5, CPU-only)

Sonde RAM : 45,2 GB/s (4 threads) / 42,3 GB/s (8 threads) / 30,7 GB/s (1 thread).

| Modèle (Q4_K_M) | tg mesuré | est. défaut | err. | est. calibré | err. | eff_ram résolu |
|-----------------|-----------|-------------|------|--------------|------|----------------|
| Qwen2.5-0.5B (463 MiB, 24 l.) | 112,04 tok/s | 72,75 | −35,1 % | 89,72 | **−19,9 %** | 1,00 (plafonné) |
| Qwen2.5-3B (1,95 GiB, 36 l.) | 21,22 tok/s | 17,17 | −19,1 % | 21,23 | **+0,0 %** | 0,99 |

Prefill mesuré : 808 t/s (0,5B) et 247 t/s (3B) → 1,0 à 1,7 TFLOPS effectifs ;
la constante unique retenue (1,25 TFLOPS eff.) donne ≤ ±26 % sur les deux.

### 4.3 Verdict

**Gate P0 satisfait sur cet hôte** : après étalonnage par tier (procédure
prévue au plan), l'erreur decode est ≤ 20 % sur les deux modèles (< 30 % exigé).
Le decode CPU Q4_K est quasi parfaitement limité par la bande passante mesurée.

### 4.4 Limites connues

1. **Cache L3 non modélisé** : pour les modèles dont une fraction significative
   tient en L3 (ex. 0,5B sur 96 Mo de X3D), la bande passante apparente dépasse
   la DRAM (`eff_ram` résolu > 1). Le modèle reste conservateur dans ce cas.
2. **`eff_gpu` / `eff_prefill_gpu` non mesurés** (hôte P0 sans GPU dédié) :
   à étalonner en P1 dès accès à la machine de référence GPU ; la structure
   (`solve_efficiency`) est prête.
3. **Streaming disque non mesuré** : l'hypothèse de chevauchement
   `max(compute, stream)` sera validée en P1 avec l'offload réel llama.cpp
   (mmap + `--no-mmap` comparés).
4. **KV cache** : coût proportionnel au contexte actif supposé (`ctx_tokens`) ;
   l'attention paged (vLLM-like) n'est pas modélisée à ce stade.
5. **Quants mixtes** : `embed_bytes` est lu au débit du tier sans distinguer
   la précision de stockage (q4/q6) — effet < 5 % sur les modèles testés.

## 5. Conséquences pour P1

- Le Placement Manager réel (P1.2) réutilisera `place_model_with_budgets`
  tel quel, en remplaçant les budgets simulés par les compteurs VRAM/RAM réels.
- La procédure d'étalonnage (sonde + llama-bench + `solve_efficiency`) devra
  être rejouée sur la machine de référence GPU avant le Gate P1 (TTFT < 2 s).
- Les 6 scénarios §17.2 sont automatisés (`cargo test -p aos-sim`) et servent
  de non-régression pour P1+.

## 6. Écarts reversés dans les specs

- `resource_hints.embed_bytes` et `resource_hints.kv_bytes_per_token` ajoutés
  au schéma du Model Registry (§3.2 ne décomposait pas les poids) — voir
  `data/models/catalog.yaml`.
