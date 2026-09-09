# AK-022 — Erreur de génération média ambiguë

- Priorité : P1
- Statut : corrigé, déployé et vérifié

Après interruption d’un `sd.exe` vidéo, le fil affichait « Impossible de charger ce
modèle », alors qu’il s’agissait d’un échec de génération. Le contrôleur UI remplace
désormais les erreurs `media.image.generate` par un message dédié et localisé,
réactive le formulaire et évite d’exposer les chemins de poids.
