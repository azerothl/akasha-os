# Phase P5 — GPU first-class + polish

**Langue :** [English](../../phases/phase-p5.md) | Français


## Objectif

Continuous batching (NFR-04), polish, et (hors hôte 1 GPU) multi-GPU /
aarch64 / `AccelDevice` fer nu. **Sortie visée : Agent OS v1.0.**

## Livrables

| # | Livrable | État |
|---|----------|------|
| P5.1 | Continuous batching | `LlamaContext::generate_batch` + dispatcher `n_seq_max=8` |
| P5.2 | Multi-GPU pipeline | Écart : 1 GPU sur l'hôte de dev |
| P5.3 | AccelDevice natif | Fer nu (ADR 0001), pas l'hôte Windows |
| P5.4 | UI avancée | Reporté (egui déjà choisi, TUI v1) |
| P5.5 | aarch64 | Reporté (pas de machine ARM64 ici) |
| P5.6 | Stabilisation | Gate P5.1 + docs |

## Gate

```powershell
.\demo\run-demo.ps1 -Gate p5
```

Critère bloquant sur cet hôte : 8 flux simultanés, wall ≤ 1,25× l'unitaire
(ou tok/s moyen ≥ 80 % du unitaire). Multi-GPU n'est pas bloquant (1 GPU).

Mesure (12/08/2026, RTX 4080 SUPER, Qwen2.5-3B Q4) : unitaire **216 ms /
134 tok/s** ; 8 flux **8/8 en 168 ms (×0,77 wall)**.

## Écarts (honnêtes)

- P5.2 : `llama_max_devices` = max de compilation (16), pas le nombre de
  GPU physiques (1 ici). `split_mode=layer` est déjà posé.
- P5.3 : `AccelDevice` seL4 = produit fer nu, pas le scaffold hôte.
- P5.4 / P5.5 : UI avancée et aarch64 hors matériel / scope immédiat.

## Statut

- P5.1 : **terminé** (gate hôte)
- P5.2–P5.5 : **écarts documentés** (matériel / fer nu)
- Cible produit : **fer nu** — ADR 0001
