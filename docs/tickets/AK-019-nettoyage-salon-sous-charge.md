# AK-019 — Nettoyage terminal d’un salon sous charge

- Priorité : P2
- Statut : planifié

L’annulation du tour répond immédiatement depuis AK-011. Il reste à mesurer le
nettoyage du conducteur et des workers quand plusieurs tours sont lancés/cancelés
en parallèle, puis à publier un événement terminal `cancelled` avec l’identifiant
du tour et un objectif p95 inférieur à une seconde.
