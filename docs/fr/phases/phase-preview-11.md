# Phase P11 — Preview 0.11.0 (E20 decode local)

**Langue :** [English](../../phases/phase-preview-11.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.11.0** : **E20 leviers de decode local** sur
le chemin llama.cpp existant — KV Q8, prefix cache sérieux (`llama_state_*`),
et speculative prompt-lookup pour les jobs mono-flux (C1). **Pas** de vLLM /
DFlash2 / second GGUF draft.

Dépend de P10. Pas un nouveau numéro P6. Pas fer nu. Pas fermeture cohorte PC.

Priorités : [plan-evolutions.md](../plan-evolutions.md) **E20**.

## Livrables

| # | Évolution | Livrable | Statut |
|---|-----------|----------|--------|
| P11.1 | E20 KV | `LoadOptions.kv_type` Q8_0 (GPU+flash-attn) / F16 ; Placement `kv_bytes_typed` | fait |
| P11.2 | E20 préfixe | prefill suffixe `memory_seq_rm` + warm `llama_state_*` ; restore E18 fail-closed | fait |
| P11.3 | E20 lookup | draft n-gram Rust + verify ; dispatcher C1 seul ; N>1 = batch P5.1 | fait |
| P11.4 | E20 métriques | `ModelMetrics.draft_accept` / `prefix_hit` ; ligne UI E5 | fait |
| P11.5 | Docs | FEATURES / STATUS / TESTER / roadmap EN+FR | fait |

## Comportement

```text
model.infer → dispatch_loop
  ├─ batch size 1 → generate_lookup (réutilisation préfixe + draft n-gram)
  └─ batch size ≥2 → generate_batch_admit (P5.1, inchangé)
```

- Speculative **exact** (même sampler ; rejet → `memory_seq_rm`).
- KV Q4 **pas** défaut (conflit flash-attn CUDA) ; Q8 seulement.
- Pas de feature `common` / MTP / DFlash2 dans le TCB.

## Gates de sortie

| Gate | Critère |
|------|---------|
| Unit | tests `common_prefix_len` + `prompt_lookup_draft` (sans GPU) |
| P5.1 | `aos-gate-p5` toujours vert |
| E18 | migrate fail-closed vers rejeu texte si restore d'état échoue |
| Smoke | C1 chat / `generate` stream toujours des tokens |

## Hors scope

Serving vLLM, second GGUF draft, feature `common`, speculative multi-seq,
Q4 KV par défaut, marketing 381 tok/s.

## E21 — emprunts inspirés FreeToken (pas une dépendance)

| # | Idée | Livrable | Statut |
|---|------|----------|--------|
| E21.1 | Signaux bande passante | `aos-placement::bandwidth` — RAM mesurée (`host_probe` 256 Mio), lookup GPU + PCIe gen×largeur via `nvidia-smi` ; écrit dans `var/run/hardware.json` ; `ModelSubsystem` → `HardwareProfile` ; decode hybride utilise `host_to_device_bw` | fait |
| E21.2 | Ancres sémantiques de préfixe | `aos-llama::semantic` — ancrage E20 `prepare_seq0_prefix` aux marqueurs tour/outil/pensée ChatML ; tests unitaires | fait |
| E21.3 | LRU expert MoE | **Non implémenté** — bindings llama.cpp sans tenseurs par expert ; documenté dans [moe-expert-offload.md](../moe-expert-offload.md) | documenté |

Inspiré de [arXiv:2608.16157](https://arxiv.org/abs/2608.16157) (FreeToken). Akasha OS **n’intègre pas** FreeToken, vLLM, SGLang ni un second moteur.
