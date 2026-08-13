# Piste VM seL4 — échafaudage noyau (ADR 0001)

## Objectif

Booter **seL4** (Microkit) sous QEMU `virt` aarch64, **sans GPU**, et
rejouer les critères testables du gate P4 : caps natives via IPC seL4
(pas TCP), révocation immédiate, arrêt d'Audit sans tuer CapKernel.

Ce n'est **pas** le produit (fer nu). C'est l'échafaudage noyau, en
parallèle de l'hôte Windows+CUDA.

## Invité

| PD | Rôle | Isolation |
|----|------|-----------|
| `capkd` | mint / check / revoke via `CapStore` | domaine passif, prio 200 |
| `bus` | lookup + proxy `cap.*` | domaine passif, prio 150 |
| `gate` | client du gate PV | prio 100, parent d'`auditd` |
| `auditd` | journal minimal | enfant ; `pd_stop` depuis le gate |

Transport : `microkit_ppcall`. Le gate **n'appelle pas** capkd
directement — tout transite par `bus` (PV.2). Contrat : `abi.h` /
`aos-sel4-abi`. La table de caps n'est plus dupliquée en C : le PD
appelle `aos-sel4-capkd` (`aos-caps::CapStore`).

## Gate

```powershell
.\demo\run-sel4-vm.ps1
```

Prérequis : WSL distro **Ubuntu**, sudo apt, `rustup target add
aarch64-unknown-none`. Le SDK Microkit 2.3.0 est téléchargé dans
`vm/sel4/sdk/` (gitignoré). `run-sel4-vm.ps1` construit d'abord la
staticlib Rust, puis l'image Microkit.

Succès : le serial QEMU contient `AOS_GATE_VM_PASS`.

## Écarts (honnêtes)

- Glue Microkit (`init` / `protected`) encore en C ; le magasin de caps
  est le `CapStore` Rust. PDs 100 % Rust (`sel4-microkit`) = suite.
- Pas de bus TCP dans l'invité (remplacé par PPC) — c'est le but.
- Pas de Model / llama.cpp (CPU-only, ADR 0001).
- Microkit est statique : « kill » = `microkit_pd_stop` du PD enfant,
  pas `SIGKILL` d'un processus hôte.
