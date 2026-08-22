# MoE — LRU par expert : limitation (E21.3)

**Langue :** [English](../moe-expert-offload.md) | Français

> Statut : limitation documentée (Preview 0.11.x)  
> Inspiration : [arXiv:2608.16157](https://arxiv.org/abs/2608.16157) (FreeToken) — **non intégré**  
> Backend : `aos-llama` / llama.cpp uniquement (pas vLLM, SGLang, FTW, second moteur)

## Question

Akasha OS peut-il implémenter un **LRU de working-set par expert MoE** (promotion/démotion d’experts individuels entre RAM hôte et GPU) sur le chemin llama.cpp actuel ?

## Preuve via l’API liée (`llama-cpp-sys-2` 0.1.154)

Audit des bindings générés (`llama_model_*`, `llama_get_*`, tenseurs) :

| Capacité | Disponible ? | Notes |
|----------|--------------|-------|
| Offload GPU par couche (`n_gpu_layers`) | Oui | Blocs transformer entiers, pas par expert |
| Tenseurs par expert (`llama_model_get_tensor`, id expert) | **Non** | Aucun symbole `expert` / `moe` dans les bindings |
| Contrôle runtime de résidence expert | **Non** | Routage MoE interne à llama.cpp |
| État KV / séquence (`llama_state_*`, `llama_memory_seq_rm`) | Oui | E20 prefix cache — niveau séquence, pas poids experts |

Le GGUF peut contenir des métadonnées MoE (`llama_model_meta_*`), mais le TCB Preview n’expose pas d’API pour épingler, évincer ou LRU des tenseurs de poids d’experts.

## Ce que Placement fait déjà pour les modèles MoE

- Packs MoE tagués (`moe`) et placés comme modèles denses : shards de couches VRAM/RAM/DISK via `aos-placement`.
- Plans hybrides : bande passante **hôte↔device** (E21) quand les couches sont réparties RAM+VRAM.
- Granularité **couche**, pas expert.

## Décision

**Hors scope Preview 0.11.x** sans fork llama.cpp ou second moteur d’inférence (interdit).

Travail futur (si llama.cpp expose des hooks d’offload par expert) :

1. Mapper les experts en `ShardKind::Expert(id)` dans le Placement Manager.
2. LRU sur `last_use_tick` par shard expert (même mécanisme que les couches dans `PlacementSim`).
3. DMA via `host_to_device_bw` mesuré dans `hardware.json`.

En attendant : offload de couches llama.cpp + budgets VRAM/RAM suffisants.

## Voir aussi

- [plan-evolutions.md](plan-evolutions.md) **E21**
- [phases/phase-preview-11.md](phases/phase-preview-11.md) §E21
- [adr/0002-model-placement.md](../../adr/0002-model-placement.md)
