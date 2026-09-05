# ADR 0002: Model Placement Algorithm

**Language:** English | [Français](../docs/fr/adr/0002-model-placement.md)

## Context

Phase P0 validated the RAM/GPU/disk placement algorithm and the capability
model. This ADR formalizes the technical specifications of the placement
algorithm for use in all later phases.

## Specifications

### Goal
Produce a realistic placement plan that optimizes resource use (RAM, VRAM,
disk) while respecting latency and bandwidth constraints.

### Inputs
- **Model**: size (parameter count), layer count, architecture type
  (Transformer, LFM, etc.)
- **Hardware**: VRAM, RAM, memory bandwidth, GPU/NPU capacity
- **Constraints**: max acceptable latency (TTFT < 2s), energy budget

### Outputs
- **Placement plan**: layer assignment to buffers (CPU, GPU, RAM, NVMe)
- **Performance estimate**: tokens/s (TTFT), memory use, I/O latency
- **Validation**: comparison with real llama.cpp measurements
- **Adaptive decision**: `InferencePlan` selects a registered local backend,
  safe quantization, KV policy, thermal policy and explicit fallback chain.
  Experimental NPU/WebGPU/LAN adapters are disabled by default.

### Success metrics
- TTFT estimate < 2s for models > 32B parameters
- Memory use ≤ 80% of available VRAM
- I/O latency < 10ms for disk accesses
- Privacy constraints respected (data locality)

### Implementation
- **Components**: PlacementManager (interface), Allocator, Profiler
- **Technologies**: Rust, llama.cpp FFI, criterion benchmarks
- **Tests**: automatic placement scenarios (6 scenarios in
  `docs/technical-specs.md` §17.2)

## Risks

- **Inaccurate performance estimates**: optimal assignment depends heavily on
  real hardware topology. Mitigation: continuous profiling and dynamic
  adjustment.
- **Resource conflicts**: multiple agents requesting the same resources.
  Mitigation: hierarchical allocation with priority.

## References

- [Technical specs — Section 17.2](../docs/technical-specs.md)
- [P0.1 Placement Manager simulator](../docs/development-plan.md)
