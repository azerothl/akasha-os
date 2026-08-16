# Fonctionnalités Preview — Akasha OS 0.3.0

**Langue :** [English](../FEATURES.md) | Français

Catalogue des fonctionnalités **livrées** sur l'hôte Windows/Linux.
Ce n'est **pas** l'OS bootable. Les exigences v1 sont dans
[specs-fonctionnelles.md](specs-fonctionnelles.md) ; les gates dans
[STATUS.md](STATUS.md).

> Date : 16/08/2026 · Preview **0.3.0**

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
| Téléchargement GGUF | Offerings `share/models/catalog-offerings.json` → `share/models/` |
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
- Réponses streamées du modèle local (offline par défaut)
- **Cartes agent** dans le chat quand un agent de fond est lié (`/agent` ou délégation)
- Injection `mem.context` avant infer (hits session + faits utilisateur)

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

---

## 3. Mémoire (PC.7)

- **Faits utilisateur** long terme : remember / recall dans l'onglet Mémoire
- Hits session + user assemblés en `mem.context` pour le chat et les agents
- Mémoire épisodique agent (`mem.episodic_*`) et `memory.remember` / `memory.recall`
- **Bootstrap mémoire d'abord** : avant les outils, le runtime fait `task.assess` puis un `mem.bootstrap` ciblé pour réutiliser les faits connus avant d'aller sur le web

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

- Backends locaux unifiés (llama.cpp CUDA **ou** CPU) + remote OpenAI-compatible optionnel (P3)
- Routage : **local_only** (défaut) ou **balanced** (Settings)
- Onglet Models : lister / charger / télécharger ; défaut de session
- Métriques live : TTFT, tok/s, VRAM/RAM/disque (barre latérale + Models)
- Bandeau vert si de nouveaux packs correspondent au tier VRAM
- CLI : `aos-session --download-models <id>…`
- Continuous batching (`generate_batch`, `n_seq_max=8`) sur l'hôte (P5.1)

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
| Réseau | Autoriser le réseau (même interrupteur que la barre latérale) |
| Agents | Modèle par défaut, max steps (1–128), timeout (60–86400 s) |
| Schedules | Intervalle de déclenchement agent (`schedule.*`) |
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
| Site | [azerothl.github.io/akasha-os](https://azerothl.github.io/akasha-os/?lang=fr) — tableau à volets, EN/FR |

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

## 11. Hors Preview 0.3.0

- Image bootable / fer nu
- macOS
- Application automatique complète des updates (téléchargement maintenant, apply au prochain lancement)
- Génération audio / vidéo native
- Marketplace public de modules
- Canaux de messagerie (Slack/Discord/etc.) dans le noyau OS
- Comptes multi-utilisateur simultanés
- Pipeline multi-GPU complet (P5.2 ; hôte mono-GPU)

Protocole cohorte : [TESTER.md](TESTER.md).
