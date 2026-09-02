# Fonctionnalités Preview — Akasha OS 0.15.0

**Langue :** [English](../FEATURES.md) | Français

Catalogue des fonctionnalités **livrées** sur l'hôte Windows/Linux/macOS.
Ce n'est **pas** l'OS bootable. Les exigences v1 sont dans
[specs-fonctionnelles.md](specs-fonctionnelles.md) ; les gates dans
[STATUS.md](STATUS.md).

> Date : 29/08/2026 · Preview **0.15.0**

### Nouveautés 0.15.0

### Contrôles d’optimisation de l’inférence

`modeld` accepte un bloc `inference_optimization` avec `prefix_cache` et `speculation`
(`auto`/`on`/`off`), un budget configurable de drafts prompt-lookup,
`min_spec_priority` et `adaptive_batching`. Les replis sûrs restent actifs et
le dernier mode d’inférence est exposé avec les métriques `draft_accept` et
`prefix_hit`.

- **Guides in-app** : ? Chat à côté du nom de session ; ? Canvas après Tout effacer — Créer avait déjà un guide
- **Planifications Chat** : en cours / pause (Reprendre | Arrêter) / arrêt sans quitter la session
- **Document recherche** : dans le fil Répondre | Préparer un document — progression, Prêt, listé sous Plus → Documents
- **Bibliothèque utilisateur** : pdf/txt/md consultatifs (état vide + ligne)
- **Offre skill du matin** : le titre nomme le besoin ; invite atténuée ; Créer | Plus tard — jamais de création auto

### Nouveautés 0.14.0

- **Agents personnalisés** : le libellé conversation / thread est le Nom de la page Agents ; le sélecteur salon liste les personas du roster et les agents Task/personnalisés sans lignes persona en double
- **Guide Créer** : guide in-app EN+FR depuis le ? dans l’en-tête Créer — prompt, photo de référence, Corriger une zone, vidéo, taille/qualité, Générer, prompt négatif, LoRA (liste du modèle), VAE (défaut du pack), Composition sur le canevas de résultat

### Nouveautés 0.13.0

- **Image de référence** : passer une image de référence lors d'une génération dans Créer — guide le style et la composition
- **Corriger une zone** : peindre un masque sur une image générée et régénérer uniquement cette zone (Créer → Corriger une zone)
- **Courte vidéo** : générer des clips de 2 à 4 secondes depuis Créer quand un pack Wan ou LTX est installé
- **Documents chat** : joindre un PDF, txt ou md depuis le même trombone que les images ; le modèle lit le texte en contexte

### Nouveautés 0.12.1

- **Chat template Gemma 4** : repli Jinja via hf-chat-template (minijinja) quand `llama_chat_apply_template` retourne UNKNOWN sur les templates tool-calling intégrés — corrige les échecs infer « chat template » sur Gemma 4 E4B (texte et vision)
- **Gate** : coordinateur `aos-gate-gemma4-vision` (infer texte G1, prefill PNG/mtmd G2) ; voir [gemma4-vision-gate.md](../gemma4-vision-gate.md)

### Nouveautés 0.12.0

- **Chat vision** : `InferRequest.images` pour chemins PNG/JPEG locaux ; sidecars mmproj catalogue pour profils vision Gemma / Qwen ; `generate_with_images` sur le chemin llama ; trombone / puces image UI ; refus si le modèle de session n'a pas la vision
- **Outils canvas** : outils agent avec conscience de scène ; couleurs de trait `set_style` live dans le module WASM ; corrections fan-out draw
- **macOS Apple Silicon** : zip Preview sur GitHub Releases (modeld Metal + CPU) ; troisième bande mill ; build non signé — note Gatekeeper sur le tooltip Mac seulement

### Nouveautés 0.11.0

- **E20 decode local** : cache KV **Q8_0** sur GPU (F16 sur CPU) via `LoadOptions` ; Placement utilise des octets KV typés
- **Prefix cache** : prefill suffixe (`memory_seq_rm`) + warm `llama_state_*` — TTFT plus bas au tour 2 / restore migrate E18 (fail-closed)
- **Speculative prompt-lookup** en mono-flux (C1) seulement ; batch N>1 = chemin P5.1 ; sampler exact (rejet → `memory_seq_rm`)
- **Métriques** : `draft_accept` / `prefix_hit` sur la ligne Models / barre latérale
- **E21** (même release) : bande passante RAM mesurée + GPU/PCIe dans `hardware.json` ; ancrage sémantique du préfixe aux marqueurs ChatML tour/outil/pensée. LRU par expert MoE documenté hors scope

### Nouveautés 0.10.1

- **Parité bridge E8** : `aos-bridged` expose toute la table `mem.*` en live (+ secrets) ; binaire dans `bin/` du zip Preview ; smoke `mem.stats` / `mem.list`
- **Téléchargement auto des updates** (opt-in, Settings) : archive dans `var/updates/` ; bandeau « prête — relancer » ; apply toujours au prochain lancement
- **Widgets E15** : `pie` et `scatter` (hôte egui ; toujours pas de webview)
- **img2img** : options fermées `init_image` + `strength` sur `media.image.generate` ; studio Image ; inpaint/mask hors scope

### Nouveautés 0.10.0

- **E7 TPM** : la clé maître du vault préfère un vrai scellage TPM hôte si dispo (Platform Crypto Win `NCrypt` / `TPM_RSA_SRK_SEAL_KEY`) ; sous Linux, fallback keyring/fichier tant que le scellage tpm2 n’est pas branché. La seule présence d’un TPM ne force pas `master.backend=tpm`. Pas de scellage PCR
- **E8 bridge live** : binaire optionnel séparé `aos-bridged` — HTTP loopback `/v1` JSON↔CBOR vers le bus (mem + secrets.list ; secrets.get/set style service). Pas dans `aos-session`
- **E9 multi-GPU** : plumbing Placement / llama `tensor_split` ; gate P5 **skip** sur 1 GPU (STATUS honnête — hard-green = run 2 GPU)
- **Polish Media/UX** : canvas **composition** studio, **upscale** RealESRGAN / `media.image.upscale`, expert DiT ; Wan/LTX = **expérimental** (pas requis TESTER). Pas d’UI vidéo produit
- **Historique image** : le studio recharge les sidecars PNG (`*.meta.json`) — prompt, prompt enrichi, composition
- **Chat UX** : bulles user / assistant distinctes ; fil plus lisible
- **RAG produit** : au boot, `aos-platformd` indexe `docs/FEATURES|STATUS|TESTER` (+ `fr/`) dans `product:docs` ; chaque `mem.context` ramène top-k chunks (budget plafonné) pour que l’assistant réponde aux questions UI / changelog sans coller tout le catalogue dans le prompt système
- **seL4 interne** : tag `sel4-pv-0.10.0` + CI QEMU — hors zip testeur / `latest.json`

### Nouveautés 0.9.0

- **Migrate mid-token** (E18) : Settings **auto / gpu / cpu** appelle `model.migrate` — la complétion live continue sur le même stream (pas de Stop, pas de tour cancelled). Sur NVIDIA le pin **cpu** reste sur le binaire CUDA `aos-modeld` avec `n_gpu_layers = 0`. Fallback fail-closed = cancel+restart 0.8 (audité)
- **Options média fermées** (E19) : `media.image.generate` / `media.audio.generate` portent un objet `deny_unknown_fields` (taille, steps, CFG, seed, sampler, négatif, knobs Piper). Clés inconnues refusées et auditées. `aos-sd` ne mappe que des flags allowlistés
- Packs optionnels extra : Flux2-class, Ideogram4-class, Piper `en_GB` ; `extra_files` VAE / CLIP / T5 / LoRA
- Settings / Models : pack image et voix Piper par défaut ; `/image` et les tools les honorent après restart
- Onglet **studio Image** ; bouton **Ouvrir dans le studio** sur un PNG du chat
- **Carte TTS** : `/speak` ouvre voix + knobs puis **Générer**

### Nouveautés 0.8.0

- **Image + TTS** (E16) : `media.image.generate` / `media.audio.generate` écrivent PNG/WAV sous `/downloads` ; cap `media.generate` ; le Placement Manager comptabilise la VRAM média
- Slash chat `/image` / `/speak` : l’image s’affiche, le clip se joue ; widgets E15 `image` / `audio` sur le même chemin
- Packs média optionnels (`local:sd-v1-5`, Piper `en_US` / `fr_FR`) — téléchargement Models, **pas** dans le zip ; first-run ne les tire pas. Le même Download installe le moteur sd.cpp / piper dans `bin/` s’il manque
- **Artefact hôte unifié** (E17) : un zip Win + un tar Linux ; `aos-modeld` (CUDA) + `aos-modeld-cpu` ; Settings **auto / gpu / cpu** redémarre modeld dans la session
- **Désinstall module** (F-MOD-01) : liste des modules installés ; confirm ; révocation `tool.invoke:<name>` ; refus des bundlés
- **Widgets E15** : `form` typé (JSON Schema), `select` / `radio` / `checkbox` / `textarea` / `bar_chart` / `image` / `audio`
- Onglet **Providers** (F-MDL-04) : cloud OpenAI-compat + loopback ; clés dans le coffre ; combo Chat local vs provider
- Install one-liner : `irm https://azerothl.github.io/akasha-os/install.ps1 | iex` / `curl -fsSL https://azerothl.github.io/akasha-os/install.sh | sh`

### Nouveautés 0.7.0

- **Hôte d’UI de module déclarative** (E15) : les modules installés avec `ui.mode=declarative_ui` obtiennent un onglet dynamique dans la barre latérale — pas de webview, pas de nouvel onglet egui codé à la main par module
- Arbre de widgets fermé dans `ui/index.html` (`type: declarative_ui`) : `column`, `row`, `heading`, `text`, `markdown`, `stat_row`, `table`, `line_chart`, `bar_chart`, `pie`, `scatter`, `form`, `button`, `select` / `radio` / `checkbox` / `textarea`, `image`, `audio`
- Intent **`module.ui`** : platformd valide le document (fail-closed) ; l’hôte lie les résultats d’outils et route boutons/formulaires via la même revue de caps que `module.invoke`
- **`module.scaffold`** : champ optionnel `ui` JSON ; package/compile copient un vrai arbre (défaut : heading + form + table sur le premier outil)
- Export JSON Schema : [`docs/bridge/aos-proto-decl-ui.json`](../bridge/aos-proto-decl-ui.json)
- Onglets Notes et Tasks restent codés à la main ; `notes`, `tasks` et `ext-rt` exclus des onglets dynamiques
- Chat **« crée un module »** lance un agent (filet hôte si le modèle dump du JSON d’UI au lieu de `agent.spawn`)
- Scénarios : lancer un agent pour scaffolder / packager / installer un module script

### Nouveautés 0.6.0

- **Schémas pont sibling** (E8) : export JSON Schema de `mem.*` / `secrets.*` sous [`docs/bridge/`](../bridge/)
- **Contrat** HTTP JSON ↔ intents CBOR dans [sibling-bridge.md](sibling-bridge.md) (pas de daemon live)
- **Keyring OS** pour la clé maître du vault (E7) : Credential Manager / Secret Service ; fallback fichier 0600 (`AOS_SECRETS_FILE_KEY=1`)
- **Catalogue local signé** (E10) : `share/modules/catalogue.yaml` + ed25519 ; vérif hash à l'install ; liste / Installer dans Settings. Source extra opt-in : index Git signé `community/catalogue.yaml` (cache hors ligne ; même revue de caps ; altération refusée).
- Chat **Stop** annule le `model.infer` en cours ; **Copier** sur les messages et le corps Dépannage

### Nouveautés 0.5.0

- **Mémorisation auto depuis le chat** (E14) : option Settings (**activée** par défaut)
- Après chaque tour, `mem.extract` locale basse priorité → faits durables
- Persist via `mem.user.remember` + auto-lien E6 `updates`/`supersedes`
- Filtre secrets : clés API / tokens / IBAN-like jamais auto-stockés (audité)
- Badge **`[chat]`** dans la liste Mémoire ; toast à l'écriture
- Non bloquant (coalescé) ; skip si un extract précédent tourne encore
- **`user.ask`** : question en cours de tâche dans le chat lié ; l'agent attend
  (`Blocked`), reprend à la réponse (FIFO si plusieurs attendent ; timeout 10 min)

### Nouveautés 0.4.0

- Graphe mémoire typé (`similar` / `updates` / `supersedes`) + auto-lien
- Onglet Mémoire : lister / éditer / supprimer / superséder ; bootstrap structuré
- Coffre à secrets chiffré (Settings) ; interpolation MCP `${secret:name}`
- Install module : revue de caps obligatoire ; refus → quarantaine
- `share/mcp/servers.yaml.example` ; `latest.json` liste CUDA **et** CPU
- Doc pont sibling ([sibling-bridge.md](sibling-bridge.md))

### Nouveautés 0.3.0

- Métriques d'inférence live (TTFT, tok/s, VRAM) dans la barre latérale + Models
- Onglet Caps : lister par détenteur + révoquer (audité)
- Boot CPU-only + pack first-run `cpu` (NVIDIA optionnel)
- Scheduler agent (`schedule.*`) + UI Settings
- Module **tasks** dual-surface + onglet Tasks
- Artefacts de packaging CUDA et CPU
- Budget de contexte agent + retry `PromptTooLong` ; garde anti-boucle JSON tronqué
- Chat : markdown des actions ; popup `/` au-dessus de l'input ; thèmes
- Briefs spawn courts ; découpage notes.create / notes.update

### Nouveautés 0.2.0

- Site public refait en tableau à volets (EN/FR)
- Le module notes empaqueté se resynchronise au boot après une update Preview
- **Dépannage** in-app : collecte un diagnostic et peut ouvrir un rapport GitHub
- `notes.read` accepte `title`, `name`, `path` ou `slug`

---

## 1. Premier lancement

| Fonctionnalité | Rôle |
|----------------|------|
| NVIDIA optionnel | Sans GPU, démarre en CPU-only (lent OK) ; `AOS_REQUIRE_GPU=1` ou Settings→GPU pour refuser |
| Contrôles disque | Refuse de démarrer sans espace suffisant |
| Sonde matériel | Écrit `var/run/hardware.json` (VRAM, RAM, disque) ; tier inclut **cpu** |
| Choix des modèles | Pack auto-best selon le tier (`cpu` / low / mid / high) |
| Téléchargement GGUF | Offerings `share/models/catalog-offerings.json` → `share/models/` ; packs média tirent aussi `bin/sd` / `bin/piper` |
| Tutoriel in-app | Onboarding 4 étapes (langue, confiance, routage, scénarios) |
| Boot ordonné | bus → capkd → auditd → modeld → platformd → agentd → egui |

Tiers :

- **cpu** (sans NVIDIA) : Qwen3.5-4B + Embedding 0.6B
- **low** (&lt;10 Go VRAM) : Qwen3.5-4B + Embedding 0.6B
- **mid** (10–20 Go) : Qwen3.5-9B + Embedding 0.6B
- **high** (≥20 Go) : Qwen3 30B-A3B + Embedding 0.6B

Voir [FIRST-RUN.md](FIRST-RUN.md).

---

## 2. Chat et sessions (PC.6)

- Conversations **parallèles persistées** ; l'historique survit au redémarrage
- **Modèle par session** (sinon le modèle instruct par défaut)
- Réponses streamées du modèle local (offline par défaut) ; **Stop** annule l'inférence ; **Copier** sur les messages
- **Cartes agent** dans le chat quand un agent de fond est lié (`/agent` ou délégation)
- Injection `mem.context` avant infer (hits session + faits utilisateur)
- **Mode Room** (tranches 1–2 backend, tranche 3 UI) : `ChatSessionMode::Room` étend la même
  `ChatSession` avec membres de salon et `speaker_id` sur les lignes du transcript.
  Tranche 2 : **RoomConductor** dans `aos-agentd` (`chat.session.room.turn` →
  `agent.room_conduct` / `agent.room_turn`). Tranche 3 : UI Chat (onglet existant) :
  bande membres, **Activer le salon**, personas (Researcher / Critic / Coder / Planner),
  autocomplétion `@` sur le roster, libellés bulles via roster, indicateur de réflexion + annulation,
  onglet Agents **Ajouter à la session** (+ case à cocher join-on-create si salon actif).
  Pas de canal Telegram/Discord — distinct des canaux messagerie
  (voir [sibling-bridge.md](sibling-bridge.md)).

Commandes slash :

| Commande | Action |
|----------|--------|
| `/commands` | Liste des commandes |
| `/help` | Instantané système (services, agents, modèles, mémoire, audit) |
| `/agent <tâche>` | Agent en fond ; carte dans la session courante |
| `/notes` | Lister les notes |
| `/notenew <titre> \| <contenu>` | Créer une note |
| `/notesearch <requête>` | Recherche sémantique dans les notes |
| `/audit [n]` | *n* derniers événements d'audit |
| `/kill <id>` / `/pause <id>` | Contrôler un agent |
| `/image <prompt>` | Générer un PNG (`media.image.generate`) sous `/downloads` |
| `/speak <texte>` | Générer un WAV (`media.audio.generate`) sous `/downloads` |
| `/canvas` | Basculer le canvas de dessin vectoriel partagé de la session |

**Canvas chat (traits vectoriels, live)** — surface de dessin liée à la session (humain + agents). Humain : Sélect. (déplacer / supprimer / aligner / rotation), crayon, ligne, courbe, silhouette (`path`), rectangle, ellipse ; calques nommés (masquer / verrouiller / opacité) ; grille et aimant 0.01 ; export PNG, SVG ou JSON. Agents : module WASM `canvas.*` (`stroke` / `path` / `rect` / `ellipse` / `move` / `delete` / `align` / `rotate` / `layer_*` / `get` / `export`). Un validateur vectoriel global CPU-only contrôle la géométrie et la topologie toutes les trois mutations et avant la fin ; la vision reste facultative. Le seau (`canvas.fill`) n’est pas dans la barre humaine — flood-fill PNG seulement. Document : `var/sessions/<id>/canvas.json`. Distinct du studio Image et de `/image`.

---

## 3. Mémoire (PC.7 + P04.1/P04.2 + P05 / E14)

- **Faits utilisateur** long terme : remember / recall / lister / éditer / supprimer
- Relations typées : `similar`, `updates`, `supersedes` (auto-lien)
- Hits session + user assemblés en `mem.context` pour le chat et les agents
- Mémoire épisodique agent (`mem.episodic_*`) et `memory.remember` / `memory.recall`
- **Bootstrap mémoire d'abord** : `task.assess` puis `mem.bootstrap` structuré (faits actifs + voisins similar ; supersédés omis)
- **Mémorisation auto depuis le chat** (Settings, **activée** par défaut) : après chaque tour,
  `mem.extract` propose des faits durables → `mem.user.remember` avec `source=chat` ;
  secrets filtrés ; badge **`[chat]`** dans la liste Mémoire

---

## 4. Notes (P2.6)

- Module WASM double-surface (`notes.aospkg`)
- UI humaine : créer / lister / rechercher
- Outils agent : `notes.create`, `notes.update`, `notes.search`, …
- Les mêmes données pour humains et agents
- Au boot, `share/modules/notes.aospkg` est copié vers `var/modules/notes` si le hash du manifeste ou l'empreinte WASM diffère (une update Preview ne doit pas garder un module périmé)
- `notes.read` accepte `title`, `name`, `path` ou `slug`

---

## 4b. Tasks (P03.5 / E3)

- Module WASM dual-surface (`tasks.aospkg`)
- UI humaine : onglet **Tasks** — créer / lister / compléter
- Outils agent : `tasks.create`, `tasks.list`, `tasks.update`, `tasks.complete`
- Même store JSON pour humains et agents (`/documents/tasks/tasks.json`)
- Resync au boot comme notes ; skill `tasks` livrée sous `share/skills/`

---

## 5. Agents

Boucle Observe / Think / Act avec caps, confirmation et audit.

| Fonctionnalité | Détail |
|----------------|--------|
| Boucle de goal | `goal.complete` / `goal.fail` ; max steps et timeout (défauts Settings) |
| `task.assess` | Classe le goal en **simple** ou **complex** ; complex active la skill planner |
| Skills | Recettes déclaratives (`share/skills/`, surchargeables sous `var/skills/`) |
| Outils | Natif, module WASM, MCP, ou runtime (plan / spawn / mémoire) |
| MCP | Serveurs stdio optionnels (`share/mcp/servers.yaml.example`) |
| Sous-agents | `agent.spawn` / `agent.await` avec un brief étroit |
| `user.ask` | Pause et question à l'utilisateur dans le chat lié ; réponse via steer |
| Hot-grant | `cap.request` sous trust + confirmation |
| Authoring | `skill.create` ; WASM scripté via `ext-rt` (`module.scaffold` / `package`) |
| Think Qwen | Blocs hybrides `<think>` retirés des prompts et de l'UI |

Skills livrées : **notes-writer**, **research**, **file-author**, **planner**, **tasks**.

### Scheduler (P03.4 / E2)

- Intents `schedule.create` / `schedule.list` / `schedule.cancel` (système ; pas les canaux chat)
- Persist sous `var/schedules/` ; ticker déclenche les agents à intervalle (min 30 s)
- UI Settings : créer / lister / annuler

### Panneau de transparence (F-UI-04 / F-UI-05)

**Détail** agent (onglet Agents ou carte chat) :

- État live, tour *n/max*, tokens, durée, badge simple/complex
- Skills et serveurs MCP ; parent / enfants
- **Sources** (web / document / fetch) avec liens navigateur
- Timeline (`agent.trace`) : action, args, résultat, type d'outil (native / module / mcp / runtime)
- Contrôles : **Pause**, **Reprendre**, **Relancer**, **Kill**, **Steer**

---

## 6. Modèles

- Backends locaux unifiés (llama.cpp CUDA **ou** CPU) + **Providers** nommés (P08.12 / F-MDL-04)
- Routage : **local_only** (défaut) ou **balanced** (Settings)
- Onglet Models : lister / charger / télécharger ; défaut de session
- Métriques live : TTFT, tok/s, VRAM/RAM/disque (barre latérale + Models)
- Bandeau vert si de nouveaux packs correspondent au tier VRAM
- CLI : `aos-session --download-models <id>…`
- Continuous batching (`generate_batch`, `n_seq_max`) sur l'hôte (P5.1)
- **E20 decode (0.11) :** KV Q8_0 sur GPU (F16 sur CPU) ; réutilisation de
  préfixe + warm `llama_state_*` (TTFT tour 2 / migrate E18) ; speculative
  prompt-lookup en **mono-flux** seulement (batch N>1 inchangé)
- Métriques live : ratio `draft` et tokens `préfixe` optionnels

### Image + TTS (E16)

- Intents `media.image.generate` / `media.audio.generate` ; cap `media.generate` ; audit ; fichiers sous `/downloads`
- Packs optionnels (`local:sd-v1-5`, `local:piper-en-us`, `local:piper-fr-fr`) — **pas** dans le zip ; first-run les saute
- **Télécharger un pack tire aussi le moteur** (`bin/sd` / `bin/piper` + DLL / `espeak-ng-data`) s’il manque ; le boot répare le même trou
- Sans poids **ou** sans moteur, Preview écrit un PNG stub visible / un WAV court pour tester le pipeline
- Le Placement Manager traite les poids média comme shards évincables vs le LLM chargé

### Providers (F-MDL-04)

- Onglet **Providers** : ajouter / lister / tester / retirer cloud OpenAI-compat et loopback (Ollama / vLLM / LM Studio)
- Les clés API vivent dans le vault, jamais dans le fichier provider
- Combo Chat : local vs provider ; `local_only` autorise encore le loopback ; le WAN demande **balanced** + Autoriser le réseau
- Un infer `secret` reste local

---

## 7. Réseau (opt-in)

Mode par défaut : **`offline_strict`** (egress refusé). Activer
**Autoriser le réseau** dans la barre latérale ou Settings.

| Intent | Rôle |
|--------|------|
| `web.search` | Multi-moteurs : `auto` (Brave → DuckDuckGo → Bing), ou forcer `brave` / `duckduckgo` / `bing` |
| `web.browse` | HTML → texte (sans JavaScript) ; `max_chars` configurable |
| `net.fetch` | Télécharger une URL dans le FS logique (défaut `/downloads/`) |
| `files.generate` | Écrire `md` / `txt` / `json` / `csv` / `png` / `pdf` |

Secrets optionnels (`var/secrets/keys.yaml`) :

```yaml
keys:
  brave_search_api_key: "BSA..."
  github_token: "ghp_..."
```

Sans clé Brave, `auto` bascule sur DuckDuckGo puis Bing HTML.

---

## 8. Settings

Persistés dans `var/run/preferences.json` (migration depuis `onboarding.json` si besoin).

| Groupe | Options |
|--------|---------|
| Général | Langue **en** / **fr** ; trust **low** / **medium** ; Inférence **auto** / **gpu** / **cpu** |
| Modèles | Routage `local_only` / `balanced` |
| Réseau / Mémoire | Autoriser le réseau ; **Mémorisation auto depuis le chat** (E14, **activée** par défaut) |
| Agents | Modèle par défaut, max steps (1–128), timeout (60–86400 s) |
| Schedules | Intervalle de déclenchement agent (`schedule.*`) |
| Secrets | Clés Brave / GitHub / OpenAI → vault chiffré ; clé maître dans le keyring OS |
| Modules | Catalogue local signé (E10) ; index Git communautaire opt-in ; l'install demande toujours la revue de caps ; désinstall des non-bundlés (pas `notes` / `tasks` / `ext-rt` / `canvas`) |
| Web | Moteur de recherche, max caractères browse, max octets fetch |

---

## 9. Audit, politique, caps, retours, mises à jour

| Domaine | Contenu |
|---------|---------|
| Audit | Journal append-only hashé ; onglet Audit ; tuer `aos-auditd` → le superviseur le relance |
| Caps | Onglet Caps : `cap.list` par détenteur ; révocation (audité) |
| Confirmation | Bandeau bloquant pour les actions sensibles ; timeout = refus (fail-closed) |
| Retour | Copie locale `var/feedback/` + issue GitHub optionnelle (security reste local) |
| Dépannage | Diagnostic in-app (NVIDIA, home, logs) ; ouvre un rapport GitHub s'il y a des anomalies |
| Updates | Bandeau si une Release plus récente existe ; overlay `bin/` + `share/` sans toucher `var/` ni écraser `etc/*.yaml` |
| Site | [azerothl.github.io/akasha-os](https://azerothl.github.io/akasha-os/?lang=fr) — planétaire, EN/FR |

---

## 10. Primitives de sécurité (hôte)

Déjà sur l'hôte Preview (P0–P4), pas seulement « prévues » :

- Capacités logiques puis natives (`aos-caps` / `aos-capkd`)
- IPC sémantique (CBOR, intents typés)
- Sandbox WASM (wasmtime) + injection de caps
- Politique déclarative, egress deny-by-default, trust manager
- Daemons isolés + auditd autonome

Piste VM seL4 (PV.1–PV.3) séparée : [phases/phase-vm-sel4.md](phases/phase-vm-sel4.md).

---

## 11. Hors Preview 0.15.1

- Image bootable / fer nu
- STT / voix permanente
- APIs natives Messages/Gemini/Bedrock (Providers OpenAI-compat seulement)
- Store public / payant de modules (le catalogue Git signé opt-in n’est pas un clone de ClawHub)
- Canaux de messagerie (Slack/Discord/etc.) dans le noyau OS
- Comptes multi-utilisateur simultanés
- Multi-GPU **hard-green** sans run 2 GPU documenté (chemin + skip 1 GPU en 0.10)
- Scellage PCR / attestation vault
- Merge binaire sibling / assistant-as-module
- Webview sandboxée / UI module HTML/JS (compositor E13)
- kind `webview` (pie/scatter livrés en 0.10.1)
- Second GGUF draft / vLLM dans le TCB / DFlash2 (E20 = prompt-lookup seulement)
- Guest seL4 dans le zip Preview public (`sel4-pv-*` interne seulement)

Protocole cohorte : [TESTER.md](TESTER.md).
