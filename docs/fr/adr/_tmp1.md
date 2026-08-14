# ADR 0001: Microkernel et caps natives (P4)

## Cible produit

**Agent OS est l'OS de la machine.** Le d├®ploiement vis├® est une machine
qui boot le microkernel (seL4) et les services Agent OS, **sans Windows
ni Linux en dessous**. L'h├┤te de dev et la VM QEMU sont des **├®chafaudages**,
pas la forme livr├®e.

## Contexte

La phase P4 devait porter les services userspace valid├®s (P0ÔÇôP3) sur un
microkernel capability-based (seL4 ou Redox). Le plan de d├®veloppement
pr├®voit une **d├®cision structurante** : si le gate P4 s'av├¿re trop co├╗teux
(drivers GPU/NPU), rester sur l'h├┤te **en v1 de d├®veloppement** tout en
conservant l'architecture capability-based ÔÇö sans abandonner la cible
bare-metal.

L'h├┤te de d├®veloppement est Windows (RTX 4080S). seL4 n'y a pas de
bring-up GPU utilisable. On s├®pare donc : GPU sur l'h├┤te, noyau dans une
VM, puis **fusion sur fer nu**.

## Options (P4 v1 seulement)

### A ÔÇö Port seL4 r├®el d├¿s P4
Preuve formelle, caps kernel natives, IPC seL4. Co├╗t : bring-up, drivers
userspace, toolchain hors Windows. Bloqu├® par le GPU en P4.

### B ÔÇö Port Redox d├¿s P4
Meilleur fit Rust, virtio plus accessible. Toujours un OS invit├® ├á
maintenir, pas de llama.cpp/CUDA natif.

### C ÔÇö Noyau de caps userspace + processus isol├®s (h├┤te)
`aos-capkd` est le **point d'application de confiance unique** : mint,
derive, grant, revoke, check. Une r├®vocation y est imm├®diatement globale
pour tous les processus (v├®rifi├®e via le bus). Les services essentiels
tournent d├®j├á en processus s├®par├®s (Model, Agent, Storage/Policy,
Audit, CapKernel). L'IPC s├®mantique transporte `cap://kernel/<id>` dans
l'enveloppe. Le transport reste TCP localhost ; la s├®mantique est celle
du bus natif.

## D├®cision

**P4 v1 = option C** (├®chafaudage sur l'h├┤te). **Cible finale = seL4
bare-metal** (option A, plus tard). Redox n'est pas retenu, sauf si le
Rust userspace seL4 bloque trop.

Raisons du report, pas de l'abandon :
- le goulot P4.1 (drivers GPU) est r├®el sur cet h├┤te Windows ;
- les crit├¿res **testables** du gate P4 sont d├®montrables sans changer
  de noyau ;
- figer une stack GPU virtio jetable avant d'avoir un `AccelDevice`
  natif (P5.3) retarderait le bare-metal au lieu de l'approcher.

## Cons├®quences

- **P4.2** : `aos-capkd` remplace les `CapStore` locaux pour l'acc├¿s
  ressources (fs). Les caps logiques P1ÔÇôP3 restent valides en fallback
  (modules WASM, agents) tant qu'aucune URI `cap://kernel/<id>` n'est
  pr├®sent├®e.
- **P4.3** : le Semantic IPC Bus n'est pas r├®├®crit ; les caps natives
  voyagent dans l'enveloppe. Transport seL4 = piste VM puis fer nu.
- **P4.4** : `aos-auditd` est un processus autonome ; le tuer n'affecte
  ni Model ni UI (forward fire-and-forget).
- **P4.5** : UI = TUI/egui sur l'h├┤te (compositor microkernel = fer nu).
- **P4.6** : boot offline = `demo/run-demo.ps1 -Gate p4` (pas de r├®seau).
- **P5 h├┤te** : GPU first-class (batching, multi-GPU) **sur l'├®chafaudage
  Windows**, pour ne pas attendre le fer nu.
- **Apr├¿s P4** : piste VM puis fer nu ÔÇö voir ci-dessous.

## Pistes : ├®chafaudage ÔåÆ produit

Le port noyau et le GPU first-class **ne partagent pas le m├¬me v├®hicule
pendant l'int├®gration**. `virtio-gpu` est de l'affichage, pas CUDA ; un
passthrough RTX 4080 Super depuis Windows vers un invit├® seL4 n'est pas
un chemin s├®rieux. WSL2 donne du CUDA, mais c'est du Linux userspace.

| Piste | R├┤le | O├╣ | Objectif |
|-------|------|----|----------|
| **H├┤te** | ├®chafaudage | Windows + CUDA | Inf├®rence, scheduler GPU, gates P1ÔÇôP5 mesurables |
| **VM** | ├®chafaudage noyau | QEMU, invit├® seL4 **sans GPU** | Boot, caps kernel, IPC seL4 ├á la place du TCP, gate P4 rejou├® CPU-only |
| **Fer nu** | **produit** | Machine qui boot seL4 + Agent OS, **aucun autre OS** | Caps + IPC natives, puis `AccelDevice` GPU (P5.3), offline-first |

Ordre : **H├┤te et VM en parall├¿le** (contrat = bus s├®mantique
`cap://kernel/<id>` ; la VM ne change que le transport), **puis fer nu**
quand le boot seL4 + services essentiels sont verts en VM. Sur le fer :
d'abord les m├¬mes services CPU-only que la VM, ensuite le GPU natif
(pas virtio).

Dans l'invit├® VM : image seL4 `virt` sous QEMU (Microkit), d'abord
`capkd` + `auditd` + gate (CPU-only), rejeu du gate P4 via
`.\demo\run-sel4-vm.ps1`. Voir `vm/sel4/README.md` et `phase-vm-sel4.md`.

## R├®f├®rences

- `plan-developpement-phases.md` ┬ºP4, **┬ºPV**, ┬ºP5.3 (`AccelDevice` natif)
- [seL4](https://sel4.dev/), [Redox OS](https://redox-os.org/)
- `specs-techniques.md` ┬º2.3 (caps), ┬º2.4 (IPC), ┬º1.3 (userspace d'abord)
