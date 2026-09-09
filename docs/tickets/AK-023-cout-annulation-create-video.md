# AK-023 — Coût et annulation de la génération vidéo

- Priorité : P1
- Statut : partiellement corrigé, déployé et vérifié

La génération UI du preset LTX 3 s/49 frames a affiché une progression correcte,
mais a nécessité environ 13 minutes avant interruption manuelle de la recette.
Le formulaire expose maintenant le nombre de frames envoyé au moteur et un bouton
Arrêter. L’intent `media.image.cancel` propage l’annulation au processus sd.cpp ;
le fichier temporaire est supprimé et aucun artefact partiel n’est publié.

Critères d’acceptation :

- annoncer le nombre de frames et une estimation (ou une fourchette) avant lancement ;
- rendre l’annulation disponible pendant la génération et libérer le job ;
- afficher un état « annulé » distinct d’une erreur de chargement de modèle ;
- conserver les sorties partielles hors du transcript et nettoyer le fichier temporaire.

Vérification live : clip LTX 256×256 / 65 frames lancé via le bus, annulation après
3 secondes (`media.image.cancel`, réponse 0 ms), erreur `génération annulée`, et
aucun fichier `/downloads/akasha-cancel-test-2.webm` persistant.

La fourchette temporelle calibrée sur plusieurs GPU reste à instrumenter ; le
produit affiche désormais une estimation prudente sous forme d’intervalle,
pondérée par le profil fast/balanced/quality, sans la présenter comme une
mesure matérielle exacte.
