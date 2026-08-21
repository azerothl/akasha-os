# Phase P5 — GPU first-class + polish

**Langue :** [English](../../phases/phase-p5.md) | Français


## Objectif

Continuous batching (NFR-04), polish, et (hors hôte 1 GPU) multi-GPU /
aarch64 / `AccelDevice` fer nu. **Sortie visée : Agent OS v1.0.**

## Livrables

| # | Livrable | État |
|---|----------|------|
| P5.1 | Continuous batching | `LlamaContext::generate_batch` + dispatcher `n_seq_max=8` |
| P5.2 | Multi-GPU pipeline | **Chemin code Preview 0.10** (`tensor_split` + partition) ; hard-green = ≥2 GPU |
| P5.3 | AccelDevice natif | Fer nu (ADR 0001), pas l'hôte Windows |
| P5.4 | UI avancée | Reporté (egui déjà choisi, TUI v1) |
| P5.5 | aarch64 | Reporté (pas de machine ARM64 ici) |
| P5.6 | Stabilisation | Gate P5.1 + docs |

## Gate

```powershell
.\demo\run-demo.ps1 -Gate p5
```

Critère bloquant sur cet hôte : 8 flux simultanés, wall ≤ 1,25× l'unitaire
(ou tok/s moyen ≥ 80 % du unitaire). Multi-GPU est **skip** (pas un fail)
si `gpu_device_count() < 2` ; pass/fail seulement sur hôte ≥2 GPU.

Mesure (12/08/2026, RTX 4080 SUPER, Qwen2.5-3B Q4) : unitaire **216 ms /
134 tok/s** ; 8 flux **8/8 en 168 ms (×0,77 wall)**.

## Écarts (honnêtes)

- P5.2 : Preview **0.10** livre le chemin layer-pipeline
  (`LoadOptions.tensor_split` / `main_gpu`, `HardwareProfile` multi-GPU,
  partition Placement Manager, gate avec vrai `gpu_device_count`). Sur 1 GPU
  le critère multi-GPU est **SKIP** — ne pas revendiquer P5.2 terminé sans
  run 2-GPU documenté. `llama_max_devices` reste le max de compilation.
- P5.3 : `AccelDevice` seL4 = produit fer nu, pas le scaffold hôte.
- P5.4 / P5.5 : UI avancée et aarch64 hors matériel / scope immédiat.

## Statut

- P5.1 : **fait** (gate hôte)
- P5.2 : **plumbing fait (0.10)** ; validation HW reportée
- P5.3–P5.5 : **écarts documentés** (matériel / fer nu)
- Cible produit : **fer nu** — ADR 0001
