# Phase P4 — Native caps and service isolation

**Language:** English | [Français](../fr/phases/phase-p4.md)

## Goal

Port microkernel semantics (native caps, semantic IPC, isolated services) onto
the host without depending on a seL4/Redox bring-up blocked by GPU drivers.
**Exit: executable P4 gate.** See ADR 0001.

## Deliverables

| # | Deliverable | Status |
|---|-------------|--------|
| P4.1 | Choice + bring-up | ADR 0001: userspace cap kernel; seL4 deferred |
| P4.2 | Native caps | `aos-capkd` — immediate global revoke |
| P4.3 | Semantic IPC | `cap://kernel/<id>` URIs in the bus envelope |
| P4.4 | Isolated services | Model, Agent, platformd, `aos-auditd`, `aos-capkd` |
| P4.5 | UI | TUI/egui on host (microkernel compositor = P5) |
| P4.6 | Offline boot | `demo/run-demo.ps1 -Gate p4` |

## Gate

```powershell
.\demo\run-demo.ps1 -Gate p4
```

Criteria: isolated services, cross-process revoke, kill Audit without impacting
Model/UI, offline assistant.

## Honest gaps

- No booted seL4/Redox microkernel (Windows host, native GPU required).
- IPC transport = localhost TCP, not seL4 primitives.
- Agent worker caps still logical (P1); FS access goes through the kernel once
  a kernel cap is presented.
- Hardware secret envelope (TPM) deferred.

## Status

- Phase P4: **done** (gate on the host)
- Product target: **bare metal** — ADR 0001
- Path: host (GPU) ∥ seL4 VM without GPU → bare-metal merge
