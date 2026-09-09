# Recette Preview — 9 septembre 2026

## Périmètre et méthode

Installation réelle : `C:\Users\azero\AppData\Local\AgentOS-Preview`.
L’application était arrêtée au début de cette reprise. Aucune mise à jour en attente ; superviseur relancé et huit processus observés.
Tests via le bus interne `127.0.0.1:24701`, client `recette-20260909`, et non par manipulation de l’interface native. Le pilotage natif n’est pas disponible dans cette session. Aucun verdict global UI/UX ne peut être donné.

Client reproductible : `crates/aos-agent/examples/preview_probe.rs`, compilé avec `cargo build -p aos-agent --example preview_probe`.
Accepte INTENT, JSON, puis délai maximal en secondes. Attention : le délai client n’annule pas nécessairement une opération côté serveur.

## Résultats exécutés

- Image : `media.image.generate`, modèle `local:sd-v1-5`, 512 × 512, 12 étapes, seed 42. Prompt : « A red ceramic teapot on a pale wooden table, soft daylight, product photography, no text ». Moteur `sdcpp`, 390731 octets, 6631 ms. PNG inspecté : théière rouge cohérente, sans texte. Un exemple réussi ne prouve pas la robustesse générale.
- Voix : `media.audio.generate`, `local:piper-fr-fr`, texte français. Moteur `piper`, 334216 octets, 1548 ms. En-tête RIFF/WAVE et fréquence 22050 Hz contrôlés ; intelligibilité et lecture UI non évaluées.
- Canvas : ouverture, rectangle rouge rempli, persistance séquence 1, export PNG 512 × 512 et sidecar JSON. Export 5000 octets en 6 ms ; rendu inspecté et conforme. Pas de validation des interactions souris ni du dessin par agent.
- Salon : création de session, passage en mode room, création et ajout d’un agent roster. Tour en 14300 ms ; une réponse française pertinente persistée avec `speaker_id=agent-191`, `cancelled=false`. Pas de test de débat multiagent.
- Vidéo : `media.image.generate`, `local:ltx2.3-dev`, `sd_mode=vid_gen`, 9 frames, 256 × 256, 4 étapes, seed 42. Retour en 29 ms, `engine=stub`, 4444 octets. Le fichier demandé en MP4 porte la signature PNG `89 50 4E 47 0D 0A 1A 0A`. ÉCHEC : aucune vraie vidéo produite.

## Défaut prioritaire : faux succès vidéo

Le catalogue contient une offre LTX et les poids sont présents sur disque, mais cela ne démontre pas que le modèle est utilisable par le registre actif. Le code inspecté `crates/aos-model/src/media.rs`, fonction `run_image`, remplace une erreur de recherche du modèle par `missing.safetensors`, puis permet la persistance d’un résultat de simulation. Le test installé reproduit cette classe de défaut. La cause exacte d’indisponibilité du modèle LTX reste à confirmer.

Recommandations : retourner une erreur explicite avec action de réparation quand le modèle manque ; réserver la simulation à un mode volontaire et clairement indiqué ; vérifier le format réel des fichiers avant de déclarer une génération réussie ; ajouter un test de non-régression pour modèle demandé absent et pour signature MP4.

## Traces conservées

Session : `sess-1788919095229`, titre « Recette 20260909 — Canvas et salon ».
Agent de recette : `agent-191` (roster).
Fichiers dans `C:\Users\azero\AppData\Local\AgentOS-Preview\var\storage\data\downloads` :

- `recette-20260909-image.png`
- `recette-20260909-voix.wav`
- `recette-20260909-canvas.png`
- `recette-20260909-canvas.json`
- `recette-20260909-video.mp4` : faux MP4 conservé comme preuve, ne pas considérer comme vidéo valide.

Pas de correction du produit effectuée. Ajout du client de recette et de ce rapport uniquement.

## À vérifier avant validation complète

UI native et accessibilité ; lecture audio/vidéo ; état de progression et annulation ; historique multimédia ; dessin par prompt ; salons multiagents ; vision ; upscale ; mémoire/RAG ; outils et permissions ; intégrations externes ; récupération après erreur. Les 629 tests ciblés précédemment réussis ne remplacent pas ces parcours réels.
