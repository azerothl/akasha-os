# Recette Preview — 9 septembre 2026

## Périmètre et méthode

Installation réelle : `C:\Users\azero\AppData\Local\AgentOS-Preview`.
L’application était arrêtée au début de cette reprise. Aucune mise à jour en attente ; superviseur relancé et huit processus observés.
Tests via le bus interne `127.0.0.1:24701`, client `recette-20260909`, complétés par inspection du code egui, le smoke-test `AOS_UI_SELF_TEST=1` et une capture Win32/PIL de la fenêtre native. Le pilotage automatisé d'un lecteur d'écran et la matrice complète de focus/DPI restent à confirmer sur un runner Windows instrumenté.

Après redémarrage, la fenêtre native `Akasha OS Preview 0.16.2` est visible (1324 × 988 px) et les huit services AOS répondent ; les clics, le focus clavier et le rendu DPI restent non instrumentés.

Client reproductible : `crates/aos-agent/examples/preview_probe.rs`, compilé avec `cargo build -p aos-agent --example preview_probe`.
Accepte INTENT, JSON, puis délai maximal en secondes. Attention : le délai client n’annule pas nécessairement une opération côté serveur.

## Résultats exécutés

- Image : `media.image.generate`, modèle `local:sd-v1-5`, 512 × 512, 12 étapes, seed 42. Prompt : « A red ceramic teapot on a pale wooden table, soft daylight, product photography, no text ». Moteur `sdcpp`, 390731 octets, 6631 ms. PNG inspecté : théière rouge cohérente, sans texte. Un exemple réussi ne prouve pas la robustesse générale.
- Voix : `media.audio.generate`, `local:piper-fr-fr`, texte français. Moteur `piper`, 334216 octets, 1548 ms. En-tête RIFF/WAVE et fréquence 22050 Hz contrôlés. La carte audio du transcript expose maintenant le format, la durée et une action `Lire/Play` (avec ouverture système secondaire) ; l’écoute subjective reste à confirmer sur une matrice audio dédiée.
- Canvas : ouverture, aspect `landscape16x9`, style de crayon (couleur/épaisseur/opacité/dash), rectangle puis ligne, exports PNG/SVG/JSON et annulation via `canvas.apply(kind=undo)` passent. L’export PNG initial (5000 octets) et le rendu ont été inspectés. L’intent direct `canvas.undo` n’est pas un service bus (le bouton UI passe bien par `canvas.apply`). Pas de validation des interactions souris ni du dessin par agent.
- Salon : session en mode room avec deux membres persistés (`agent-191`, `agent-172`). Le débat a produit deux réponses distinctes avec `speaker_id`/`speaker_name`, puis une question `user.ask` de l’agent vision ; la réponse utilisateur a permis de reprendre et clôturer le tour. Sur un tour long, `chat.session.room.turn.cancel` répond maintenant immédiatement (`elapsed_ms=0`) ; le nettoyage terminal du conducteur reste à mesurer sous charge (`AK-011`).
- Vidéo : après complétion du sidecar Gemma (SHA-256 vérifié), `sd.cpp` charge le pack LTX et une génération IPC 256×256, 9 frames, 1 étape produit `engine=sdcpp`, WebM 111629 octets avec signature EBML `1A 45 DF A3`. La carte vidéo dédiée affiche `WebM/EBML`, `0,50 s` et la taille dans le transcript, avec ouverture système. Le défaut vidéo est désormais WebM ; MP4 reste explicitement non supporté par le binaire livré (AK-001, AK-009, AK-016). `chat.session.get` confirme la persistance de la carte vidéo et de la carte audio dans la session `Audit vidéo UI`.
- Upscale : la première commande avec l’identifiant court `realesrgan-x4plus-anime` était rejetée (l’API attendait un fichier). L’alias court et ses variantes sont maintenant résolus vers `RealESRGAN_x4plus_anime_6B.pth` ; génération réelle alias réussie en 1927 ms, `engine=sdcpp`, 3048588 octets ; sortie PNG contrôlée en 2048 × 2048.
- Web : `web.search` (3 résultats) puis `web.browse` sur le site Akasha ont répondu via le bus en moins d’une seconde ; l’URL finale et le texte extrait sont cohérents. Les parcours permissions/revocation des outils restent à tester séparément.
- Fichiers : `files.generate` a créé un JSON de 26 octets dans `/downloads/recette-20260909-files.json`, puis `fs.read` l’a relu avec sa classe `private` et sa version `1`. Une lecture agent sans `fs.read:/downloads/**` a été refusée explicitement (`PermissionDenied`). Le chemin nominal et le garde-fou de capacité passent ; suppression/rollback restent à couvrir.
- Permissions : les trois inventaires `device.permission.list`, `device.usb.permission.list` et `fs.host.permission.list` répondent sans erreur (aucun grant actif sur cette installation). La révocation d’un grant réel reste à exécuter sur une machine disposant d’un périphérique/chemin accordé.
- Délégation/outils : un agent Qwen 3.5 9B autonome (`agent-192`) a exécuté `notes.create`, est passé à l’état `Done` en 2 étapes, puis la note `/documents/notes/audit-note-probe.md` a été relue via `fs.read`. La chaîne prompt → outil → artefact → confirmation est fonctionnelle ; `agent.state` expose correctement l’état final et les capacités.
- Mémoire : un second agent (`agent-193`) a exécuté `memory.remember` puis `memory.recall` sur « Audit mémoire 2026 » et a terminé `Done` en 3 étapes ; le fait a été retrouvé avec l’identifiant `5515`.
- Récupération modèle : un `model.load` volontairement inconnu renvoie une erreur explicite (`modèle inconnu`) puis un chargement Qwen 3.5 9B réussit immédiatement en profil effectif `memorysaver` (placement 0,02 Gio VRAM / 1,57 Gio RAM / 3,72 Gio disque). Le chemin erreur → reprise est sain.
- Vision : le modèle Qwen3-VL 4B et son mmproj sont chargés (`has_vision=true`). Une inférence mtmd sur `/downloads/recette-20260909-image.png` renvoie « Chaleur. » en 3,37 s ; le défaut de résolution des chemins logiques a été corrigé dans AK-015.
- Paramètres/UI : les onglets Chat, Agents, Créer, Mémoire, Modèles, Providers et Settings sont couverts par les tests de layout/état et le smoke-test `AOS_UI_SELF_TEST`; la densité confortable/compacte, les thèmes fr/en/custom et le sélecteur de modèle sont rendus dans la capture native. Les matrices DPI/lecteur d’écran doivent encore être automatisées.

## Audit technique et visuel de l’UI native (source egui)

Score indicatif, fondé sur l’inspection du code, le smoke-test headless et les captures natives normale/minimale : **13/20 — acceptable, travail significatif requis**.

- Accessibilité : **2/4**. Les contrôles ont généralement un libellé visible, mais aucun parcours lecteur d’écran/focus n’est instrumenté ; plusieurs actions utilisent des variantes `small` et les gestes Canvas ne sont pas testés à grande taille de texte.
- Performance : **3/4**. Les listes principales sont dans des zones de défilement ; `try_load_chat_image` décode l’image complète à chaque rendu de pièce jointe, sans cache explicite ni miniature persistée.
- Apparence/thème : **3/4**. Les états agents utilisent maintenant les tokens sémantiques du thème ; des couleurs spécialisées restent dans les panneaux et l’illustration Canvas.
- Conformité desktop : **3/4**. L’application est une UI egui native et ne dépend pas d’un navigateur ; les cartes média restent natives au fil et délèguent le décodage au lecteur système, avec une action de lecture audio explicite.
- Adaptivité : **2/4**. Le Canvas calcule une surface disponible, mais aucune recette fenêtre redimensionnée, DPI élevé, clavier ou multi-fenêtre n’a été exécutée.

Capture native après déploiement : le sélecteur de session et la barre de statut affichent désormais le même modèle (`local:qwen3.5-9b-instruct`), ce qui clôt AK-012. La capture confirme aussi une hiérarchie lisible (rail primaire, liste de sessions, transcript, composer et barre d’état), mais les tests de focus clavier, contraste mesuré et redimensionnement restent à instrumenter.

Une séquence clavier non destructive de huit `Tab` suivie d’`Échap` a été envoyée à la fenêtre native ; `aos-ui-egui` est resté répondant. Cela valide la stabilité du focus, sans remplacer une mesure complète de l’ordre de tabulation ni un test lecteur d’écran.

Une passe à la taille minimale `702×600` a ensuite été exécutée. Les segments essentiels restent visibles (réseau, modèle tronqué, capacités, mode, langue) et le composer conserve son bouton d’envoi ; la version et les métriques détaillées sont masquées/condensées par choix responsive. AK-014 est donc corrigé et vérifié.

Ces constats alimentent AK-002 à AK-004 et AK-009. AK-005 est corrigé par alias et AK-006 est corrigé.

## Passe globale système après correction

- Installation : huit processus (`aos-session`, bus, capkd, auditd, modeld, platformd, agentd, UI) présents après redémarrage.
- GPU : RTX 4080 SUPER, 16376 MiB ; 3799 MiB utilisés, 39 °C, pilote 591.86.
- Logs : aucun `ERROR`, `panic`, `failed` ou `invalide` dans les six journaux daemon contrôlés après redémarrage ; le catalogue signé est validé cryptographiquement.
- Espace disque : l’installation Preview est sur C: (**74,67 Gio libres**) ; le checkout E: dispose de **18,69 Gio libres** après les builds (~35,9 Gio d’artefacts `target/`). Le risque concerne surtout les prochaines compilations locales ; aucun nettoyage automatique n’a été effectué pour ne pas supprimer des artefacts utilisateur.
- Smoke-test UI installé : `AOS_UI_SELF_TEST OK min_inner=702x600 locale=fr/en theme=custom`.
- Tests checkout : 376 tests UI, 261 agent + 11 worker + 1 daemon, 29 modèle et 172 platform réussis (850 au total ; platform/modèle sans CUDA) ; `cargo check --workspace` réussi.
- Gate conversationnel historique : 7/8 critères passent ; le seul échec est le scénario 32B qui référence `local:llama-q6-32b`, absent du pack Preview installé. Ce n’est pas un échec du chemin utilisé par l’UI (Qwen 3.5 9B) mais le gate doit être paramétré sur les offres réellement installées avant d’être utilisé comme feu vert release.

## Tickets et prise en charge

- AK-001 : corrigé, déployé et vérifié — faux succès bloqué et WebM réel validé.
- AK-002 : corrigé pour Preview — libellé audio explicite et tooltip ; l’ouverture système est assumée.
- AK-003 : corrigé pour le périmètre critique — états agents centralisés sur les tokens thème.
- AK-004 : corrigé pour le périmètre CI local — `AOS_UI_SELF_TEST=1` disponible et validé ; l’automatisation native reste une dépendance de runner.
- AK-005 : corrigé et testé — alias court résolu vers le poids installé ; upscale réel vérifié en 2048 × 2048.
- AK-006 : corrigé et déployé — signature Ed25519 catalogue valide, log propre.
- AK-007 : corrigé dans le checkout — `ConfirmationResult.approved` correctement extrait ; `cargo check -p aos-platform --bin aos-platformd` passe.
- AK-008 : corrigé dans le checkout — le CLI renseigne `persistent: false` ; `cargo check --workspace` passe.
- AK-009 : corrigé, déployé et vérifié — sidecar Gemma complet installé, hash publié et vidéo WebM réelle générée.
- AK-011 : corrigé et déployé côté contrôle UI — le handler répond immédiatement et le runtime est réveillé par `Notify` ; la mesure de nettoyage terminal devient une amélioration non bloquante.
- AK-012 : corrigé, déployé et vérifié visuellement — la barre de statut suit le modèle de la session active.
- AK-013 : corrigé, déployé et vérifié — l’UI attend désormais le bus au lieu de rester vide après un démarrage devancé.
- AK-014 : corrigé, déployé et vérifié à `702×600` — la barre de statut passe en mode compact sans débordement.
- AK-015 : corrigé, déployé et vérifié — les chemins logiques `/downloads/...` sont résolus vers le stockage hôte avant vision mtmd.
- AK-016 : corrigé, déployé et vérifié visuellement — cartes audio/vidéo typées, métadonnées, lecture audio et ouverture système ; la vidéo n’est plus décodée comme PNG.
- AK-017 à AK-020 : pris en charge comme tickets d’évolution planifiés — recette native accessibilité/DPI, lecteur multimédia intégré, benchmark de nettoyage salon et révocation end-to-end nécessitent respectivement un runner ou des ressources absentes de cette installation.

## Défaut prioritaire : faux succès vidéo

Le catalogue contient une offre LTX et le registre actif expose `local:ltx2.3-dev`. Le sidecar Gemma complet est maintenant présent et hashé ; le pack est utilisable par `sd.cpp`.

Correction prise en charge et déployée : `run_image` refuse désormais les requêtes vidéo servies par `stub` et tout fichier sans signature WebM/AVI, avant persistance ; le chemin par défaut est WebM, format supporté par le binaire livré. Le test de signature vidéo, le préflight de sidecars et l’ensemble des tests `aos-model` passent.

Évolutions restantes : intégrer un lecteur vidéo/audio avec progression, pause et contrôle codec plus riche ; la carte actuelle affiche déjà les métadonnées disponibles et protège le transcript contre le faux décodage image.

## Traces conservées

Session : `sess-1788919095229`, titre « Recette 20260909 — Canvas et salon ».
Session média : `sess-1788946703149`, titre « Audit vidéo UI » ; cartes vidéo WebM et audio WAV persistées et relues via `chat.session.get`.
Agent de recette : `agent-191` (roster).
Fichiers dans `C:\Users\azero\AppData\Local\AgentOS-Preview\var\storage\data\downloads` :

- `recette-20260909-image.png`
- `recette-20260909-voix.wav`
- `recette-20260909-canvas.png`
- `recette-20260909-canvas.json`
- `recette-20260909-video.mp4` : faux MP4 historique conservé comme preuve, ne pas considérer comme vidéo valide.
- `recette-20260909-video-ltx-real.webm` : WebM réel généré par l’IPC (`111629` octets, signature EBML `1A 45 DF A3`).
- `recette-20260909-upscale-alias.png` : upscale IPC avec l’identifiant court (`3048588` octets, 2048 × 2048).
- `recette-20260909-files.json` : artefact JSON généré puis relu via `files.generate`/`fs.read` (`26` octets, classe `private`).
- `audit-note-probe.md` : note créée par délégation (`agent-192`, `notes.create`) puis relue par `fs.read`.

Le client de recette, les garde-fous vidéo, les cartes média audio/vidéo, le smoke-test UI, les tokens d’état et les signatures catalogue ont été corrigés ou déployés selon les tickets ci-dessus.

## À vérifier avant validation complète

À poursuivre : matrice instrumentée focus/DPI/lecteur d’écran, lecteur multimédia intégré (progression/pause), débat multiagent avec mesure du nettoyage sous charge, mémoire/RAG avancée, révocation de permissions, intégrations externes et récupération après erreur sous charge. Le démarrage devancé UI↔bus, la vision de base, les chemins logiques, les cartes média et le viewport minimum sont désormais couverts par AK-013 à AK-016.

## Axes d’amélioration priorisés

1. **P1 — Vidéo** : conserver WebM comme format natif du binaire livré, compléter le lecteur intégré et le contrôle durée/codecs ; le garde-fou anti-faux-succès et la carte typée sont en place.
2. **P1 — Recette native** : exécuter les clics, focus clavier, redimensionnement, DPI élevé et lecture multimédia sur un runner Windows instrumenté ; conserver `AOS_UI_SELF_TEST` comme smoke-test rapide.
3. **P1 — Stockage** : conserver le contrôle de seuil au démarrage (C: conforme, E: checkout à 18,69 Gio) et proposer une rétention transparente des artefacts de build/cache.
4. **P2 — Audio** : compléter la lecture/pause/progression dans le fil ; la lecture native et la durée WAV sont déjà exposées, avec ouverture système en action secondaire.
5. **P2 — Thème** : étendre les tokens sémantiques aux surfaces/outils spécialisés et tester le contraste en clair/sombre/custom.
6. **P2 — Salon** : mesurer le nettoyage terminal sous charge et publier un événement `cancelled` avec identifiant de tour ; le contrôle UI est déjà découplé.
