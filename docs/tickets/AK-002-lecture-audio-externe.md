# AK-002 — Le bouton Play audio ouvre le fichier hors de l’application

- Priorité : P2
- Statut : corrigé pour le périmètre Preview — libellé et comportement explicites
- Zone : `crates/aos-ui-egui/src/chat_media.rs`, `render_audio`

Le bouton utilise désormais le libellé localisé « Ouvrir le fichier » et un tooltip indiquant clairement l’ouverture dans le lecteur système. Le comportement est donc explicite et cohérent avec une pièce jointe exportée. Un lecteur intégré serait une amélioration ultérieure, pas un faux contrôle « Play ».

Amélioration ultérieure possible : intégrer lecture/pause/durée, mais elle n’est pas requise pour le parcours Preview actuel.
