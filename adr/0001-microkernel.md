# ADR 0001: Microkernel and native caps (P4)

**Language:** English | [Français](../docs/fr/adr/0001-microkernel.md)

## Product target

**Agent OS is the machine's OS.** The intended deployment is a machine that
boots the microkernel (seL4) and Agent OS services, **without Windows or
Linux underneath**. The host and QEMU VM are **scaffolds**, not the shipped
form.

## Context

Phase P4 was meant to port validated userspace services (P0–P3) onto a
capability-based microkernel (seL4 or Redox). The development plan calls for
a **structuring decision**: if the P4 gate proves too costly (GPU/NPU
drivers), stay on the host **for development v1** while keeping the
capability-based architecture — without abandoning the bare-metal target.

The development host is Windows (RTX 4080S). seL4 has no usable GPU bring-up
there. So we split: GPU on the host, kernel in a VM, then **merge on bare
metal**.

## Options (P4 v1 only)

### A — Real seL4 port in P4
Formal proof, native kernel caps, seL4 IPC. Cost: bring-up, userspace
drivers, non-Windows toolchain. Blocked by GPU in P4.

### B — Redox port in P4
Better Rust fit, easier virtio. Still a guest OS to maintain; no native
llama.cpp/CUDA.

### C — Userspace cap kernel + isolated processes (host)
`aos-capkd` is the **single trust application point**: mint, derive, grant,
revoke, check. A revoke is immediately global for all processes (verified via
the bus). Essential services already run as separate processes (Model, Agent,
Storage/Policy, Audit, CapKernel). Semantic IPC carries `cap://kernel/<id>`
in the envelope. Transport remains localhost TCP; semantics match the native
bus.

## Decision

**P4 v1 = option C** (host scaffold). **Final target = seL4 bare metal**
(option A, later). Redox is not selected unless seL4 Rust userspace blocks
too hard.

Reasons for deferral, not abandonment:
- the P4.1 bottleneck (GPU drivers) is real on this Windows host;
- **testable** P4 gate criteria are demonstrable without changing kernels;
- freezing a throwaway virtio-GPU stack before a native `AccelDevice` (P5.3)
  would delay bare metal instead of approaching it.

## Consequences

- **P4.2**: `aos-capkd` replaces local `CapStore`s for resource access (fs).
  Logical P1–P3 caps remain valid as fallback (WASM modules, agents) until a
  `cap://kernel/<id>` URI is presented.
- **P4.3**: Semantic IPC Bus is not rewritten; native caps travel in the
  envelope. seL4 transport = VM track then bare metal.
- **P4.4**: `aos-auditd` is autonomous; killing it does not affect Model or UI
  (fire-and-forget forward).
- **P4.5**: UI = TUI/egui on the host (microkernel compositor = bare metal).
- **P4.6**: offline boot = `demo/run-demo.ps1 -Gate p4` (no network).
- **P5 host**: first-class GPU (batching, multi-GPU) **on the Windows
  scaffold**, so we do not wait for bare metal.
- **After P4**: VM then bare-metal track — see below.

## Tracks: scaffold → product

Kernel port and first-class GPU **do not share the same vehicle during
integration**. `virtio-gpu` is display, not CUDA; RTX 4080 Super passthrough
from Windows into an seL4 guest is not a serious path. WSL2 gives CUDA, but
that is Linux userspace.

| Track | Role | Where | Goal |
|-------|------|-------|------|
| **Host** | scaffold | Windows + CUDA | Inference, GPU scheduler, measurable P1–P5 gates |
| **VM** | kernel scaffold | QEMU, seL4 guest **without GPU** | Boot, kernel caps, seL4 IPC instead of TCP, P4 gate replayed CPU-only |
| **Bare metal** | **product** | Machine boots seL4 + Agent OS, **no other OS** | Native caps + IPC, then GPU `AccelDevice` (P5.3), offline-first |

Order: **Host and VM in parallel** (contract = semantic bus
`cap://kernel/<id>`; the VM only changes transport), **then bare metal** when
seL4 boot + essential services are green in the VM. On bare metal: first the
same CPU-only services as the VM, then native GPU (not virtio).

In the VM guest: seL4 `virt` image under QEMU (Microkit), first `capkd` +
`auditd` + gate (CPU-only), P4 gate replay via `.\demo\run-sel4-vm.ps1`.
See `vm/sel4/README.md` and `docs/phases/phase-vm-sel4.md`.

## References

- `docs/development-plan.md` §P4, **§PV**, §P5.3 (native `AccelDevice`)
- [seL4](https://sel4.dev/), [Redox OS](https://redox-os.org/)
- `docs/technical-specs.md` §2.3 (caps), §2.4 (IPC), §1.3 (userspace first)
