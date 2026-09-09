# AK-023 — Coût et annulation de la génération vidéo

- Priorité : P1
- Statut : ouvert, évolution proposée

La génération UI du preset LTX 3 s/49 frames a affiché une progression correcte,
mais a nécessité environ 13 minutes avant interruption manuelle de la recette.
Le formulaire expose maintenant le nombre de frames envoyé au moteur ; il manque
encore un préflight de durée/ressources et un bouton Annuler qui propage réellement
la demande jusqu’au job `media.image.generate`/moteur vidéo.

Critères d’acceptation :

- annoncer une estimation (ou une fourchette) avant lancement ;
- rendre l’annulation disponible pendant la génération et libérer le job ;
- afficher un état « annulé » distinct d’une erreur de chargement de modèle ;
- conserver les sorties partielles hors du transcript et nettoyer le fichier temporaire.
