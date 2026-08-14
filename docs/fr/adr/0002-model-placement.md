# ADR 0002: Model Placement Algorithm

**Langue :** [English](../../adr/0002-model-placement.md) | Français


## Contexte

La Phase P0 a validé l'algorithme de placement RAM/GPU/disque et le modèle de capacités. Cette ADR formalise les spécifications techniques de l'algorithme de placement pour être utilisé dans toutes les phases suivantes.

## Spécifications

### Objectif
Produire un plan de placement réaliste qui optimise l'utilisation des ressources (RAM, VRAM, disque) tout en respectant les contraintes de latence et de bande passante.

### Entrées
- **Modèle** : Taille du modèle (nombre de paramètres), nombre de couches, type d'architecture (Transformer, LFM, etc.)
- **Matériel** : Capacité VRAM, RAM, bande passante mémoire, capacité GPU/NPU
- **Contraintes** : Latence maximale acceptable (TTFT < 2s), budget énergétique

### Sorties
- **Plan de placement** : Affectation des couches aux différents tampons (CPU, GPU, RAM, NVMe)
- **Estimation de performance** : Tokens/secondes (TTFT), utilisation mémoire, latence I/O
- **Validation** : Comparaison avec des mesures réelles sur llama.cpp

### Métriques de succès
- Estimation de TTFT < 2s pour les modèles > 32B paramètres
- Utilisation mémoire ≤ 80% de la VRAM disponible
- Latence I/O < 10ms pour les accès disque
- Respect des contraintes de confidentialité (data locality)

### Implémentation
- **Composants** : PlacementManager (interface), Allocator (gestion mémoire), Profiler (mesure)
- **Technologies** : Rust, FFI avec llama.cpp, benchmarks avec criterion
- **Tests** : Scénarios de placement automatiques (6 scénarios de specs-techniques.md §17.2)

## Risques

- **Estimation de performance inexacte** : L'affectation optimale dépend fortement de la topologie matérielle réelle. Mitigation : profilage continu et ajustement dynamique.
- **Conflits de ressources** : Plusieurs agents demandant les mêmes ressources. Mitigation : allocation hiérarchique avec priorité.

## Références

- [Specs techniques - Section 17.2](../specs-techniques.md)
- [P0.1 Simulateur de Placement Manager](../plan-developpement-phases.md)
