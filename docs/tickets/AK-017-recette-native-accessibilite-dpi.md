# AK-017 — Recette native accessibilité et DPI

- Priorité : P1
- Statut : planifié

Le smoke-test `AOS_UI_SELF_TEST=1`, la taille minimale `702×600` et une séquence de
`Tab` confirment la stabilité de base. Il manque une exécution Windows instrumentée
(UI Automation/lecteur d’écran) couvrant l’ordre de focus, le contraste mesuré, le
texte agrandi et une matrice DPI 100/125/150/200 %. Le ticket définit la prochaine
campagne sans masquer cette limite dans le verdict Preview.
