# ADR 0001: Microkernel et caps natives (P4)

**Langue :** [English](../../adr/0001-microkernel.md) | Français


## Cible produit

**Agent OS est l'OS de la machine.** Le déploiement visé est une machine
qui boot le microkernel (seL4) et les services Agent OS, **sans Windows
ni Linux en dessous**. L'hôte de dev et la VM QEMU sont des **échafaudages**,
pas la forme livrée.

## Contexte

La phase P4 devait porter les services userspace validés (P0–P3) sur un
microkernel capability-based (seL4 ou Redox). Le plan de développement
prévoit une **décision structurante** : si le gate P4 s'avère trop coûteux
(drivers GPU/NPU), rester sur l'hôte **en v1 de développement** tout en
conservant l'architecture capability-based — sans abandonner la cible
bare-metal.

L'hôte de développement est Windows (RTX 4080S). seL4 n'y a pas de
bring-up GPU utilisable. On sépare donc : GPU sur l'hôte, noyau dans une
VM, puis **fusion sur fer nu**.

## Options (P4 v1 seulement)

### A — Port seL4 réel dès P4
Preuve formelle, caps kernel natives, IPC seL4. Coût : bring-up, drivers
userspace, toolchain hors Windows. Bloqué par le GPU en P4.

### B — Port Redox dès P4
Meilleur fit Rust, virtio plus accessible. Toujours un OS invité à
maintenir, pas de llama.cpp/CUDA natif.

### C — Noyau de caps userspace + processus isolés (hôte)
`aos-capkd` est le **point d'application de confiance unique** : mint,
derive, grant, revoke, check. Une révocation y est immédiatement globale
pour tous les processus (vérifiée via le bus). Les services essentiels
tournent déjà en processus séparés (Model, Agent, Storage/Policy,
Audit, CapKernel). L'IPC sémantique transporte `cap://kernel/<id>` dans
l'enveloppe. Le transport reste TCP localhost ; la sémantique est celle
du bus natif.

## Décision

**P4 v1 = option C** (échafaudage sur l'hôte). **Cible finale = seL4
bare-metal** (option A, plus tard). Redox n'est pas retenu, sauf si le
Rust userspace seL4 bloque trop.

Raisons du report, pas de l'abandon :
- le goulot P4.1 (drivers GPU) est réel sur cet hôte Windows ;
- les critères **testables** du gate P4 sont démontrables sans changer
  de noyau ;
- figer une stack GPU virtio jetable avant d'avoir un `AccelDevice`
  natif (P5.3) retarderait le bare-metal au lieu de l'approcher.

## Conséquences

- **P4.2** : `aos-capkd` remplace les `CapStore` locaux pour l'accès
  ressources (fs). Les caps logiques P1–P3 restent valides en fallback
  (modules WASM, agents) tant qu'aucune URI `cap://kernel/<id>` n'est
  présentée.
- **P4.3** : le Semantic IPC Bus n'est pas réécrit ; les caps natives
  voyagent dans l'enveloppe. Transport seL4 = piste VM puis fer nu.
- **P4.4** : `aos-auditd` est un processus autonome ; le tuer n'affecte
  ni Model ni UI (forward fire-and-forget).
- **P4.5** : UI = TUI/egui sur l'hôte (compositor microkernel = fer nu).
- **P4.6** : boot offline = `demo/run-demo.ps1 -Gate p4` (pas de réseau).
- **P5 hôte** : GPU first-class (batching, multi-GPU) **sur l'échafaudage
  Windows**, pour ne pas attendre le fer nu.
- **Après P4** : piste VM puis fer nu — voir ci-dessous.

## Pistes : échafaudage → produit

Le port noyau et le GPU first-class **ne partagent pas le même véhicule
pendant l'intégration**. `virtio-gpu` est de l'affichage, pas CUDA ; un
passthrough RTX 4080 Super depuis Windows vers un invité seL4 n'est pas
un chemin sérieux. WSL2 donne du CUDA, mais c'est du Linux userspace.

| Piste | Rôle | Où | Objectif |
|-------|------|----|----------|
| **Hôte** | échafaudage | Windows + CUDA | Inférence, scheduler GPU, gates P1–P5 mesurables |
| **VM** | échafaudage noyau | QEMU, invité seL4 **sans GPU** | Boot, caps kernel, IPC seL4 à la place du TCP, gate P4 rejoué CPU-only |
| **Fer nu** | **produit** | Machine qui boot seL4 + Agent OS, **aucun autre OS** | Caps + IPC natives, puis `AccelDevice` GPU (P5.3), offline-first |

Ordre : **Hôte et VM en parallèle** (contrat = bus sémantique
`cap://kernel/<id>` ; la VM ne change que le transport), **puis fer nu**
quand le boot seL4 + services essentiels sont verts en VM. Sur le fer :
d'abord les mêmes services CPU-only que la VM, ensuite le GPU natif
(pas virtio).

Dans l'invité VM : image seL4 `virt` sous QEMU (Microkit), d'abord
`capkd` + `auditd` + gate (CPU-only), rejeu du gate P4 via
`.\demo\run-sel4-vm.ps1`. Voir `vm/sel4/README.md` et `phase-vm-sel4.md`.

## Références

- `plan-developpement-phases.md` §P4, **§PV**, §P5.3 (`AccelDevice` natif)
- [seL4](https://sel4.dev/), [Redox OS](https://redox-os.org/)
- `specs-techniques.md` §2.3 (caps), §2.4 (IPC), §1.3 (userspace d'abord)
