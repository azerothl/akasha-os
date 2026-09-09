# AK-014 — Débordement de la barre de statut à la taille minimale

- Priorité : P1
- Statut : corrigé, déployé et vérifié visuellement
- Zone : UI native egui, viewport minimum `702×600`

## Constat

La recette native à la taille minimale déclarée par le smoke-test affichait le modèle et le mode, mais tronquait la fin de la barre (jeton budget/version) faute de place dans une ligne de hauteur fixe.

## Correction

Les libellés deviennent compacts sous 900 px : modèle tronqué proprement, capacités et mode abrégés, jetons/budget ramenés à des indicateurs avec infobulle, version masquée uniquement dans ce mode. Aucun contrôle n’est supprimé ; les onglets détaillés restent accessibles par clic.

## Vérification

La suite UI passe. Une capture de l’installation à `702×600` montre les segments En ligne, Modèle (tronqué), Cap., Mode et FR sans débordement ; la version est volontairement masquée dans ce mode compact.
