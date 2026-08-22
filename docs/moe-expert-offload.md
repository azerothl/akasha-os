# MoE expert working-set / LRU — limitation (E21.3)

**Language:** English | [Français](fr/moe-expert-offload.md)

> Status: documented limitation (Preview 0.11.x)  
> Inspiration: [arXiv:2608.16157](https://arxiv.org/abs/2608.16157) (FreeToken) — **not integrated**  
> Backend: `aos-llama` / llama.cpp only (no vLLM, SGLang, FTW, second engine)

## Question

Can Akasha OS implement a **per-expert working-set LRU** (promote/demote individual MoE experts between host RAM and GPU) on the current llama.cpp path?

## Evidence from the bound API (`llama-cpp-sys-2` 0.1.154)

Audit of generated bindings (`llama_model_*`, `llama_get_*`, tensor APIs):

| Capability | Available? | Notes |
|------------|------------|-------|
| Layer-level GPU offload (`n_gpu_layers`) | Yes | Whole transformer blocks, not per-expert |
| Per-expert tensor handles (`llama_model_get_tensor`, expert id) | **No** | No `expert` / `moe` symbols in bindings |
| Runtime expert residency control | **No** | MoE routing stays inside llama.cpp |
| KV / sequence state (`llama_state_*`, `llama_memory_seq_rm`) | Yes | Used by E20 prefix cache — sequence level, not expert weights |

GGUF may contain MoE metadata (`llama_model_meta_*`), but the Preview TCB does not expose APIs to pin, evict, or LRU individual expert weight tensors.

## What Placement already does for MoE models

- MoE packs are tagged in the catalogue (`moe`) and placed like dense models: layer shards on VRAM/RAM/DISK via `aos-placement`.
- Hybrid plans already account for **host↔device bandwidth** (E21) when layers span RAM+VRAM.
- There is **no** finer granularity than a transformer layer.

## Decision

**Out of scope for Preview 0.11.x** without forking llama.cpp or adding a second inference engine (explicitly forbidden).

Future work (post-Preview, if llama.cpp gains expert-level offload hooks):

1. Map expert tensors to `ShardKind::Expert(id)` in the Placement Manager.
2. LRU on `last_use_tick` per expert shard (same machinery as layer shards in `PlacementSim`).
3. DMA via measured `host_to_device_bw` from `hardware.json`.

Until then, MoE models rely on llama.cpp’s built-in layer offload and sufficient VRAM/RAM budgets.

## Related

- [evolution-roadmap.md](evolution-roadmap.md) **E21**
- [phases/phase-preview-11.md](phases/phase-preview-11.md) §E21
- [adr/0002-model-placement.md](../adr/0002-model-placement.md)
