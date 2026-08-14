# Phase P5 — First-class GPU + polish

**Language:** English | [Français](../fr/phases/phase-p5.md)

## Goal

Continuous batching (NFR-04), polish, and (beyond single-GPU host) multi-GPU /
aarch64 / bare-metal `AccelDevice`. **Target exit: Agent OS v1.0.**

## Deliverables

| # | Deliverable | Status |
|---|-------------|--------|
| P5.1 | Continuous batching | `LlamaContext::generate_batch` + `n_seq_max=8` dispatcher |
| P5.2 | Multi-GPU pipeline | Gap: 1 GPU on the host |
| P5.3 | Native AccelDevice | Bare metal (ADR 0001), not Windows host |
| P5.4 | Advanced UI | Deferred (egui already chosen, TUI v1) |
| P5.5 | aarch64 | Deferred (no ARM64 machine here) |
| P5.6 | Stabilization | P5.1 gate + docs |

## Gate

```powershell
.\demo\run-demo.ps1 -Gate p5
```

Blocking criterion on this host: 8 concurrent streams, wall ≤ 1.25× single
(or average tok/s ≥ 80% of single). Multi-GPU is not blocking (1 GPU).

Measurement (2026-08-12, RTX 4080 SUPER, Qwen2.5-3B Q4): single **216 ms /
134 tok/s**; 8 streams **8/8 in 168 ms (×0.77 wall)**.

## Honest gaps

- P5.2: `llama_max_devices` = compile max (16), not physical GPU count (1 here).
  `split_mode=layer` already set.
- P5.3: seL4 `AccelDevice` = bare-metal product, not host scaffold.
- P5.4 / P5.5: advanced UI and aarch64 out of immediate hardware/scope.

## Status

- P5.1: **done** (host gate)
- P5.2–P5.5: **documented gaps** (hardware / bare metal)
- Product target: **bare metal** — ADR 0001
