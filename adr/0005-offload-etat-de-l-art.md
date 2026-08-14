# ADR 0005: Offloading state of the art

**Language:** English | [Français](../docs/fr/adr/0005-offload-etat-de-l-art.md)

## Context

Phase P1 validated inference with active offload (RAM+disk) for models larger
than VRAM. This ADR formalizes best practices and trade-offs for offloading.

## Current state

- **RAM offload**: used for layers that do not fit in VRAM (models > 32B)
- **Disk offload**: streaming slow layers to persistent storage
- **Combination**: fast layers stay in VRAM; slow layers are streamed
- **Performance**: TTFT < 2s for models > 32B on RTX 4080S

## Offloading strategy

### 1. Layer partitioning
- **Fast layers**: early layers (embedding, attention) → VRAM
- **Slow layers**: deep layers (decoders, heads) → RAM/Disk
- **Policy**: based on activation sizes estimated by the profiler

### 2. Memory management
- **Memory pool**: dynamic allocation in the Model Subsystem
- **Swapping**: page swap to RAM when VRAM is saturated
- **Prefetching**: anticipate accesses via the IPC cache

### 3. Disk offload
- **Format**: HDF5/Parquet for tensors, ZSTD compression
- **Streaming**: block reads aligned to the cache page
- **Checkpointing**: periodic save of intermediate states

## Trade-offs

| Aspect | Advantage | Drawback |
|--------|-----------|----------|
| **Latency** | Offload reduces VRAM pressure | Adds latency (streaming) |
| **Performance** | Enables larger models | Needs precise profiling |
| **Complexity** | Rich offload logic | Requires an integrated profiler |

## Recommendations

1. **Continuous profiler**: measure activation sizes in real time to adapt
   offload
2. **Hybridization**: some agents prefer CPU for reasoning tasks, GPU for
   inference
3. **Optimization**: compressed tensor formats (FP8, INT8) to reduce offload

## Phase impact

- **P1**: Model Subsystem with offload
- **P2**: Module Registry + Module Runtime with offload support
- **P3**: Audit trail including offload operations
- **P4**: Port services to microkernel with offload management

## References

- [P1.3 Inference Scheduler v1](../docs/development-plan.md)
- [Technical specs — Section 18](../docs/technical-specs.md)
