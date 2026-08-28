# seL4 VM track — kernel scaffolding (ADR 0001)

**Language:** English | [Français](../../docs/fr/vm-sel4-README.md)

## Goal

Boot **seL4** (Microkit) under QEMU `virt` aarch64, **without GPU**, and
replay the testable P4 gate criteria: native caps via seL4 IPC (not TCP),
immediate revoke, stop Audit without killing CapKernel.

This is **not** the product (bare metal). It is the kernel scaffold, in
parallel with the Windows+CUDA host.

## Guest

| PD | Role | Isolation |
|----|------|-----------|
| `capkd` | mint / check / revoke via `CapStore` | passive domain, prio 200 |
| `bus` | lookup + `cap.*` proxy | passive domain, prio 150 |
| `gate` | PV gate client | prio 100, parent of `auditd` |
| `dev` | framebuffer + virtio smoke (blk/net/input) | prio 125 (callee above gate) |
| `auditd` | minimal journal | child; `pd_stop` from the gate |

Transport: `microkit_ppcall`. The gate does **not** call capkd directly —
everything goes through `bus` (PV.2). Contract: `abi.h` / `aos-sel4-abi`.
The cap table is no longer duplicated in C: the PD calls `aos-sel4-capkd`
(`aos-caps::CapStore`).

## Gate

```powershell
.\demo\run-sel4-vm.ps1
```

Prerequisites: WSL distro **Ubuntu**, sudo apt, `rustup target add
aarch64-unknown-none`. Microkit SDK 2.3.0 is downloaded into
`vm/sel4/sdk/` (gitignored). `run-sel4-vm.ps1` builds the Rust staticlib
first, then the Microkit image.

QEMU attaches a **single void cut-face surface** (guest RAM, not virtio-gpu),
`virtio-blk` + `virtio-net` + `virtio-keyboard`, and injects a key via the
QEMU monitor while the `dev` PD polls.

Success: QEMU serial contains **`AOS_GATE_VM_PASS`** (P4 caps replay) and
**`AOS_GATE_VM_HW_PASS`** (fb + blk + net + input). Per-device markers:
`AOS_GATE_VM_FB`, `AOS_GATE_VM_BLK`, `AOS_GATE_VM_NET`, `AOS_GATE_VM_KBD`.

## Honest gaps

**Now exercised in CI (same `loader.img`):**

- Void cut-face framebuffer surface (`AOS_GATE_VM_FB`) — solid `#070b14` fill in guest RAM.
- Virtio-blk read/write against a raw `gate.disk` image (`AOS_GATE_VM_BLK`).
- Virtio-net MAC visibility (`AOS_GATE_VM_NET`).
- Virtio-keyboard event via QEMU monitor `sendkey` (`AOS_GATE_VM_KBD`).
- Combined marker `AOS_GATE_VM_HW_PASS` after the existing `AOS_GATE_VM_PASS`.

**Still missing:**

- Microkit glue (`init` / `protected`) still in C; the cap store is the Rust
  `CapStore`. 100% Rust PDs (`sel4-microkit`) = next.
- No TCP bus in the guest (replaced by PPC) — that is the point.
- No Model / llama.cpp (CPU-only, ADR 0001).
- Microkit is static: “kill” = `microkit_pd_stop` of the child PD, not
  `SIGKILL` of a host process.
- Framebuffer is a **smoke surface** (solid void fill), not the Preview
  compositor (E13) or a desktop shell.
- Virtio drivers are **minimal legacy-MMIO poll smoke** only — no full net
  stack, no block FS, no input routing to UI.
- PV.4 bare-metal USB boot is later; this track is QEMU/CI only.
