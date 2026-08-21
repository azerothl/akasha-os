# Phase PV — Piste VM seL4 (échafaudage noyau)

**Langue :** [English](../../phases/phase-vm-sel4.md) | Français


## Objectif

Conformément à ADR 0001 et au plan (§PV) : invité seL4 sous QEMU **sans
GPU**, rejeu du gate P4, **en parallèle de P5** (GPU hôte).

## Livrables

| # | Livrable | État |
|---|----------|------|
| PV.1 | Boot Microkit + capkd/auditd/gate | **passé** |
| PV.2 | PD `bus` (lookup + proxy `cap.*`) | **passé** |
| PV.3 | `aos-caps` `no_std` + `CapStore` dans l'invité | **passé** |
| PV.4 | Préparation fer nu | reporté |

```powershell
.\demo\run-sel4-vm.ps1
```

Serial attendu : `bus lookup cap.* OK` puis `AOS_GATE_VM_PASS`.

Le PD `capkd` est encore de la glue C Microkit (`init` / `protected`) ;
mint / check / revoke s'exécutent dans `aos-caps::CapStore` via la
staticlib `aos-sel4-capkd` (`aarch64-unknown-none`).

## Suite

- PDs Rust complets (`sel4-microkit`), plus de glue C
- Intents CBOR complets (au-delà du sous-ensemble `cap.*`)
- PV.4 : même image sur fer nu, puis `AccelDevice` (P5.3)

## Versioning d’intégration interne (Preview 0.10+)

| Canal | Tag | Produit |
|-------|-----|---------|
| Preview public | `v0.10.0` | Zips testeurs + `latest.json` |
| seL4 interne | `sel4-pv-0.10.0` (jamais `v*`) | Gate CI seulement — `sel4-vm-gate.yml` |

Le guest seL4 **n’est pas** dans le zip Preview. Les testeurs cohorte ne
lancent pas QEMU. Local : `.\demo\run-sel4-vm.ps1` ou `vm/sel4/run.sh`.
