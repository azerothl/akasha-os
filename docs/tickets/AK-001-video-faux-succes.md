# AK-001 — Faux succès lors d’une génération vidéo

- Priorité : P0 (bloquant release)
- Statut : corrigé, déployé dans Preview active, vérifié sur WebM réel
- Zone : `crates/aos-model/src/media.rs`, `run_image`

## Reproduction

Sur l’installation Preview, demander `media.image.generate` avec `sd_mode=vid_gen`, `video_frames=9`, `model_id=local:ltx2.3-dev` et une sortie `.webm` (le `sd.cpp` livré ne sait pas écrire de MP4 mono-fichier).

Résultat observé avant correction : réponse `engine=stub`, `bytes=4444`, succès IPC ; le fichier `.mp4` commence par la signature PNG `89 50 4E 47 0D 0A 1A 0A`.

## Impact

L’interface peut afficher ou archiver un artefact qui n’est pas une vidéo. Cela trompe l’utilisateur et rend les automatisations dangereuses.

## Correction prise en charge

Les requêtes vidéo refusent désormais tout moteur `stub` et tout fichier sans signature WebM/AVI valide. Le fichier temporaire est supprimé et une erreur actionnable est renvoyée avant persistance.

## Validation

Le test de signature PNG passe. Le chemin vidéo utilise maintenant WebM, seul format mono-fichier annoncé comme supporté par le `sd.cpp` livré. Une génération LTX IPC 256×256, 9 frames, 1 étape produit un WebM réel de 111629 octets, signature EBML `1A 45 DF A3`, moteur `sdcpp`.
