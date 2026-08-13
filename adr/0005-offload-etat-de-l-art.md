# ADR 0005: Offloading State of the Art

## Contexte

La Phase P1 a validé l'inférence avec offload actif (RAM+disque) pour les modèles > VRAM. Ce ADR formalise les meilleures pratiques et les compromis pour l'offloading.

## État actuel

- **Offload RAM** : Utilisé pour les couches qui ne tiennent pas en VRAM (modèles > 32B paramètres)
- **Offload Disque** : Streaming de couches lentes vers le stockage persistant
- **Combinaison** : Les couches rapides restent en VRAM, les couches lentes sont streamées
- **Performance** : TTFT < 2s pour les modèles > 32B sur GPU 4080S

## Stratégie d'offloading

### 1. Stratégie de partitionnement des couches
- **Couches rapides** : Premières couches (embedding, attention) → VRAM
- **Couches lentes** : Couches profondes (décodeurs, heads) → RAM/Disque
- **Politique** : Basée sur la taille des activations estimées par le profiler

### 2. Gestion de la mémoire
- **Memory Pool** : Allocation dynamique dans le Model Subsystem
- **Swapping** : Page swap vers RAM quand la VRAM est saturée
- **Prefetching** : Anticipation des accès via le cache IPC

### 3. Offload Disque
- **Format** : HDF5/Parquet pour les tenseurs, compression ZSTD
- **Streaming** : Lecture par blocs alignés sur la page de cache
- **Checkpointing** : Sauvegarde périodique des états intermédiaires

## Compromis

| Aspect | Avantage | Inconvénient |
|--------|----------|--------------|
| **Latence** | Offload réduit la pression VRAM | Ajoute de la latence (streaming) |
| **Performance** | Permet d'utiliser des modèles plus grands | Nécessite un profiling précis |
| **Complexité** | Logique d'offload complexe | Nécessite un profiler intégré |

## Recommandations

1. **Profiler continu** : Mesurer la taille des activations en temps réel pour adapter l'offload
2. **Hybridation** : Certains agents préfèrent le CPU pour les tâches de raisonnement, le GPU pour l'inférence
3. **Optimisation** : Utiliser des formats de tenseurs compressés (FP8, INT8) pour réduire l'offload

## Impact sur les phases

- **P1** : Implémentation du Model Subsystem avec offload
- **P2** : Module Registry + Module Runtime avec support offload
- **P3** : Audit trail incluant les opérations d'offload
- **P4** : Port des services sur microkernel avec gestion d'offload

## Références

- [P1.3 Inference Scheduler v1](plan-developpement-phases.md)
- [Specs techniques - Section 18](specs-techniques.md)
