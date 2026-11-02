# Ticket #439 — parcours MCP avec les vrais services

Le complément part de `main` (`301da1f`) après les PR #467, #468 et #469.
La façade MCP existante est conservée : allowlist exacte, permissions explicites,
identité `mcp:<AOS_MCP_FROM>`, modèles déjà disponibles et module Notes en lecture.

## Écart corrigé

`aos-auditd` répond à `audit.append` avec le numéro `u64` de l'événement.
Le client MCP tentait de décoder un booléen. L'événement était bien enregistré,
mais la réponse valide était interprétée comme une erreur et le diagnostic
annonçait à tort un service d'audit indisponible. Les deux chemins MCP utilisent
maintenant le type canonique ; le double de service du test d'intégration suit
également ce contrat.

## Recette reproductible

`mcp_media_probe` est un client JSON-RPC réel sur stdin/stdout d'`aos-mcpd`.
Il lance ses propres broker, plateforme, modèle et audit, sur un port local
éphémère, dans un nouveau répertoire temporaire. Il installe une copie validée
du module Notes avec les permissions du manifeste, puis crée une note synthétique
pour la recette. Cette préparation automatisée n'atteste pas une revue manuelle.

Les poids image/vidéo déjà présents sont liés dans ce répertoire par liens
matériels : aucune copie volumineuse ni téléchargement. Les liens sont retirés
après l'arrêt des processus détenus par le probe. Les journaux et les sorties
synthétiques restent dans le répertoire de preuve annoncé sur stderr.
L'installation Preview et ses données ne sont pas modifiées.

```text
cargo build -p aos-ipc -p aos-mcp -p aos-model -p aos-platform -p aos-auditd --no-default-features --bin aos-busd --bin aos-mcpd --bin aos-modeld --bin aos-platformd --bin aos-auditd --locked
cargo build -p aos-platform --no-default-features --example mcp_media_probe --locked
<bin-dir>/examples/mcp_media_probe <absolute-bin-dir> <absolute-sd-executable> <absolute-existing-models-dir> <absolute-repo-root>
```

Le probe exige Stable Diffusion 1.5 et les six fichiers des offerings SD 1.5/LTX
2.3 du catalogue, ainsi que `ffprobe` sur PATH. Les liens matériels nécessitent
que le répertoire temporaire et les poids résident sur le même système de
fichiers. Le backend GPU est choisi par sd.cpp ; le daemon modèle compilé sans
CUDA reste suffisant pour ce parcours média externe.

Le client vérifie :

- l'initialisation MCP et la liste des outils image, vidéo et Notes ;
- l'absence d'exposition de `notes.create`, malgré son ajout à l'allowlist de test ;
- la lecture de la note synthétique via le véritable module WASM installé ;
- trois refus sans permissions et trois refus d'usurpation, de traversée de
  chemin ou d'option inconnue ;
- une vraie image PNG, sa taille, son empreinte et ses dimensions ;
- une vraie vidéo WebM, son codec, ses dimensions, ses neuf images et sa
  fréquence avec un décodeur indépendant ;
- les neuf événements MCP, leurs six refus, l'identité de l'acteur, l'intégrité
  du journal canonique et l'absence du faux avertissement d'audit indisponible ;
- l'arrêt des processus qu'il a lancés et la libération de ses liens de poids.

## Résultat observé le 2 novembre 2026

Sur Windows et une RTX 4080 SUPER (backend média Vulkan0), les neuf appels
attendus ont réussi ou été refusés comme prévu : lecture réelle de Notes,
six refus, image et vidéo. L'image PNG fait 31 572 octets en 128 × 128 pixels ;
la vidéo WebM fait 227 179 octets, en VP8, neuf images de 128 × 128 à 8 images/s.
`ffprobe` a compté les images décodées. Le journal canonique comporte neuf
événements MCP, dont six refus, avec l'identité attendue et une chaîne valide.
Le faux avertissement d'audit indisponible est absent. Les six liens de poids
ont été libérés et tous les processus détenus ont été arrêtés.

Les huit tests unitaires et le test d'intégration MCP passent. Clippy strict
passe pour `aos-mcp` et `aos-platform`, toutes cibles, sans fonctionnalités par
défaut. Les résultats, empreintes des poids/binaires et limites de portée sont
conservés dans le [rapport JSON](evidence/2026-11-02-mcp-media-439.json).

## Portée de la validation

Les requêtes sont synthétiques, à une étape de diffusion, pour vérifier le
transport, le moteur, les fichiers et les autorisations. Elles ne constituent
pas une évaluation de la qualité visuelle, de la performance, des clients IDE
tiers, de Metal ou d'une autre machine. Le parcours de l'interface Preview
installée reste ouvert, tout comme les recettes de cohorte et de deux GPU.
Le ticket attend l'intégration de ce complément avant clôture.
