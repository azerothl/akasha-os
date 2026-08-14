# Phase PV — seL4 VM track (kernel scaffolding)

**Language:** English | [Français](../fr/phases/phase-vm-sel4.md)

## Goal

Per ADR 0001 and the plan (§PV): seL4 guest under QEMU **without GPU**,
replay the P4 gate, **in parallel with P5** (host GPU).

## Deliverables

| # | Deliverable | Status |
|---|-------------|--------|
| PV.1 | Microkit boot + capkd/auditd/gate | **passed** |
| PV.2 | `bus` PD (lookup + `cap.*` proxy) | **passed** |
| PV.3 | `aos-caps` `no_std` + `CapStore` in guest | **passed** |
| PV.4 | Bare-metal prep | deferred |

```powershell
.\demo\run-sel4-vm.ps1
```

Expected serial: `bus lookup cap.* OK` then `AOS_GATE_VM_PASS`.

The `capkd` PD is still Microkit C glue (`init` / `protected`);
mint / check / revoke run in `aos-caps::CapStore` via the
`aos-sel4-capkd` staticlib (`aarch64-unknown-none`).

## Next

- Full Rust PDs (`sel4-microkit`), less C glue
- Full CBOR intents (beyond the `cap.*` subset)
- PV.4: same image on bare metal, then `AccelDevice` (P5.3)
