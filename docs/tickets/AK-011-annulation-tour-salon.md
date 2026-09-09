# AK-011 — Annulation d’un tour de salon trop lente

- Priorité : P1 (contrôle utilisateur)
- Statut : corrigé et déployé (contrôle UI)
- Zone : `crates/aos-agent/src/room_runtime.rs`, `chat.session.room.turn.cancel`

## Constat

Un débat à deux membres fonctionne et persiste les réponses. Sur un tour long (20 points,
deux modèles locaux), `chat.session.room.turn.cancel` accepte la demande, mais l’appel du
tour reste bloqué jusqu’à l’expiration du client (180 s) et aucun message assistant marqué
`cancelled=true` n’est retourné dans ce délai.

## Correction prise en charge

Le handler d’annulation retire immédiatement le tour, positionne le drapeau et répond sans
attendre `model.cancel` ; la demande observée revient avec `elapsed_ms=0` côté bus. La boucle
d’inférence dispose en plus d’un réveil `Notify` pour interrompre un flux silencieux.

## Suite recommandée

Amélioration non bloquante restante : publier un événement terminal `cancelled` avec
identifiant de tour et mesurer le nettoyage p95 < 1 s sous charge.
