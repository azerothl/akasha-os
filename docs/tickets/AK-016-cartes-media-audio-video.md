# AK-016 — Cartes média audio/vidéo

## Constat

Les pièces jointes générées étaient présentées de façon inégale : la vidéo pouvait
être traitée comme une image et déclencher un décodage PNG invalide ; l’audio ne
montrait ni format ni action de lecture. Cela rendait les résultats multimédias
difficiles à vérifier dans le fil.

## Correction

- ajout d’une variante `ChatAttachment::Video` dans le protocole et le stockage ;
- carte vidéo dédiée dans le transcript, avec conteneur (WebM/EBML), taille et
  durée lorsque le conteneur l’expose, plus ouverture système ;
- carte audio enrichie (WAV : canaux, fréquence, profondeur, durée, taille) avec
  action `Play/Lire` et ouverture système secondaire ;
- extraction vision explicitement limitée aux images dans les salons ;
- tests unitaires de métadonnées WAV/WebM et build/re-déploiement Preview.

## Vérification

La session `Audit vidéo UI` affiche la carte vidéo WebM réelle (`0,50 s`, `111629`
octets) dans la fenêtre native `Akasha OS Preview 0.16.2`. La génération audio
Piper et son attachement ont également été injectés dans la session pour le
parcours de carte et d’action de lecture.

## Limite connue

La Preview délègue encore le décodage au lecteur système : il n’y a pas de barre
de progression ni de pause intégrée au transcript, et les codecs non exposés par
le conteneur restent indiqués comme inconnus.
