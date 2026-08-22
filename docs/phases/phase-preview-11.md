# Phase P11 — Preview 0.11.0 (E20 local decode)

**Language:** English | [Français](../fr/phases/phase-preview-11.md)

## Goal

Ship **Akasha OS Preview 0.11.0**: **E20 local decode levers** on the
existing llama.cpp path — KV Q8, serious prefix cache (`llama_state_*`),
and prompt-lookup speculative decoding for single-stream (C1) jobs.
Does **not** adopt vLLM / DFlash2 / a second draft GGUF.

Depends on P10. Not a new P6 gate. Not bare metal. Not PC cohort close.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) **E20**.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P11.1 | E20 KV | `LoadOptions.kv_type` Q8_0 (GPU+flash-attn) / F16 ; Placement `kv_bytes_typed` | done |
| P11.2 | E20 prefix | `memory_seq_rm` suffix prefill + warm `llama_state_*` ; E18 migrate restore fail-closed | done |
| P11.3 | E20 lookup | Rust prompt-lookup draft + verify ; dispatcher C1 only ; N>1 stays P5.1 batch | done |
| P11.4 | E20 metrics | `ModelMetrics.draft_accept` / `prefix_hit` ; E5 UI line | done |
| P11.5 | Docs | FEATURES / STATUS / TESTER / roadmap EN+FR | done |

## Behaviour

```text
model.infer → dispatch_loop
  ├─ batch size 1 → generate_lookup (prefix reuse + n-gram draft)
  └─ batch size ≥2 → generate_batch_admit (P5.1, unchanged)
```

- Speculative decode is **exact** (same sampler distribution; reject → `memory_seq_rm`).
- Q4 KV is **not** default (flash-attn CUDA conflict); Q8 only.
- No `llama-cpp-sys-2/common` / MTP / DFlash2 in the TCB.

## Exit gates

| Gate | Criterion |
|------|-----------|
| Unit | `common_prefix_len` + `prompt_lookup_draft` tests pass (no GPU) |
| P5.1 | `aos-gate-p5` still green (batch path untouched) |
| E18 | Migrate still fail-closed to prefix-replay text if state restore fails |
| Smoke | C1 `generate` / chat still streams tokens |

## Out of scope

vLLM serving, second draft GGUF, `common` feature, speculative inside multi-seq
batch, Q4 KV default, version marketing for 381 tok/s reproduction benches.

## E21 — FreeToken-inspired borrowings (not a dependency)

| # | Idea | Deliverable | Status |
|---|------|-------------|--------|
| E21.1 | Bandwidth signals | `aos-placement::bandwidth` — measured RAM (`host_probe` 256 MiB), GPU spec lookup + PCIe gen×width from `nvidia-smi`; written to `var/run/hardware.json` ; `ModelSubsystem` maps into `HardwareProfile` ; hybrid decode uses `host_to_device_bw` | done |
| E21.2 | Semantic prefix anchors | `aos-llama::semantic` — snap E20 `prepare_seq0_prefix` to ChatML turn/tool/think markers ; unit tests | done |
| E21.3 | MoE expert LRU | **Not implemented** — llama.cpp bindings lack per-expert tensors ; documented in [moe-expert-offload.md](../moe-expert-offload.md) | documented |

Inspired by [arXiv:2608.16157](https://arxiv.org/abs/2608.16157) (FreeToken). Akasha OS does **not** integrate FreeToken, vLLM, SGLang, or a second inference engine.
