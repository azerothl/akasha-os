# AK-024 — Workflow vidéo avancé inspiré de ComfyUI

- Priorité : P2
- Statut : partiellement pris en charge (FPS + image de départ livrés)

Le studio vidéo partage maintenant les contrôles sûrs déjà présents pour l’image
(prompt enrichi, négatif, seed, sampler, CFG, steps, résolution, profil, styles,
LoRA/VAE et leviers backend). Le contrat `MediaImageOptions` reste volontairement
fermé : il ne peut pas accepter un graphe ComfyUI ou des argv arbitraires.

Pour aller plus loin sans fragiliser le sandbox, ajouter au contrat et au moteur
des options typées et validées : codec, keyframes ou
trajectoire caméra, contrôle temporel, guidance vidéo et nœuds de conditionnement.
Chaque option devra être déclarée par le catalogue du pack et ignorée/refusée
explicitement si le moteur sélectionné ne la supporte pas.

Livré dans cette passe : `MediaImageOptions.fps` (borné 1–120, transmis à
`sd.cpp --fps`) et un parcours UI vidéo pour joindre/réutiliser une image de
départ avec réglage de force (`--init-img`/`--strength`). Le codec reste celui
produit par le moteur (WebM) ; les graphes/nœuds ComfyUI et keyframes restent
hors contrat tant qu'ils ne disposent pas d'un backend catalogue explicite.

Les presets vidéo sont désormais enrichis par `video_defaults` dans le catalogue
(`width`, `height`, `fps`, `frames`, bornes de résolution et durée maximale).
Les valeurs LTX 2.3 et Wan 2.2 ont été renseignées à partir de leurs model cards
Hugging Face : LTX utilise notamment 768×512, 24 fps et des frames `8n+1` ; Wan
utilise 832×480, 16 fps et des frames `4n+1`. Les prochaines versions peuvent
être ajoutées au catalogue sans modifier l'UI.
