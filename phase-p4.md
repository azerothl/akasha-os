# Phase P4 - Caps natives et isolation de services

## Objectif

Porter la sémantique microkernel (caps natives, IPC sémantique, services
isolés) sur l'hôte de dev, sans dépendre d'un bring-up seL4/Redox bloqué
par les drivers GPU. **Sortie : gate P4 exécutable.** Voir ADR 0001.

## Livrables

| # | Livrable | État |
|---|----------|------|
| P4.1 | Choix + bring-up | ADR 0001 : noyau de caps userspace ; seL4 reporté |
| P4.2 | Caps natives | `aos-capkd` — révocation globale immédiate |
| P4.3 | IPC sémantique | URIs `cap://kernel/<id>` dans l'enveloppe du bus |
| P4.4 | Services isolés | Model, Agent, platformd (Storage/Policy), `aos-auditd`, `aos-capkd` |
| P4.5 | UI | TUI/egui sur l'hôte (compositor microkernel = P5) |
| P4.6 | Boot offline | `demo/run-demo.ps1 -Gate p4` |

## Gate

```powershell
.\demo\run-demo.ps1 -Gate p4
```

Critères : services isolés, révocation cross-process, kill d'Audit sans
impact Model/UI, assistant offline.

## Écarts (honnêtes)

- Pas de microkernel seL4/Redox booté (hôte Windows, GPU natif requis).
- Transport IPC = TCP localhost, pas les primitives seL4.
- Caps d'agents workers encore logiques (P1) ; l'accès fs passe par le noyau
  dès qu'une cap kernel est présentée.
- Enveloppe hardware des secrets (TPM) reportée.

## Statut

- Phase P4 : **terminée** (gate sur l'hôte de dev)
- Cible produit : **fer nu** (machine sans autre OS) — ADR 0001
- Chemin : hôte (GPU) ∥ VM seL4 sans GPU → fusion bare-metal
