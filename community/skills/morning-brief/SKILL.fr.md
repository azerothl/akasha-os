---
name: morning-brief
description: Briefing local court depuis la mémoire, les tâches ouvertes et les notes — sans réseau
license: MIT
when_to_use: >
  L’utilisateur demande un briefing du matin, un récap quotidien, « qu’est-ce
  que je fais aujourd’hui », ou de se remettre à jour. Pas pour une recherche
  web ni un nouveau plan de projet.
tools:
  - memory.recall
  - tasks.list
  - notes.list
  - notes.search
  - goal.complete
---
# Briefing du matin

**Langue :** [English](SKILL.md) | Français

Un briefing **local**. N’utilise pas le réseau.

1. `memory.recall` pour les préférences durables (langue de l’UI, ce que
   « aujourd’hui » veut dire pour cet utilisateur). Une ou deux requêtes
   étroites, pas tout le dump mémoire.
2. `tasks.list` — items ouverts seulement. Ne **crée** ni ne **termine**
   aucune tâche.
3. `notes.list`, ou `notes.search` avec une requête courte (`today`, cette
   semaine, ou une préférence de l’étape 1). Si le module notes manque,
   saute et dis-le.
4. Rédige le briefing : **8 lignes courtes maximum**. Les faits d’abord.
   Une liste vide reste vide — n’invente ni travail, ni événements, ni
   citations.
5. N’appelle **pas** `web.search`, `web.browse` ni `net.fetch`.
6. Ne fais **pas** `notes.create` ni `tasks.create` sauf si l’utilisateur
   l’a demandé dans ce tour.
7. `goal.complete` avec le briefing comme résultat.
