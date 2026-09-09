# AK-003 — Couleurs d’état et du Canvas dispersées dans le code UI

- Priorité : P2
- Statut : corrigé pour le périmètre critique — états agents centralisés
- Zone : `crates/aos-ui-egui/src/agent_panel.rs`, `chat_canvas.rs`

Les couleurs d’état des agents passent désormais par les tokens sémantiques du thème (`WARNING`, `SUCCESS`, `ICE_TRACK`, `SIGNAL`, `PAPER`). Des couleurs spécialisées restent dans les panneaux et l’illustration Canvas.

Impact : contraste et cohérence difficiles à garantir entre thèmes, et maintenance coûteuse.

Les couleurs spécialisées de l’illustration et des cartes restent intentionnelles. Une extension des tokens de surface/outillage peut améliorer la maintenance, mais ne bloque plus la lisibilité des états.
