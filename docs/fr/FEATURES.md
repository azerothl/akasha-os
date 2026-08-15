# Fonctionnalités Preview — Akasha OS 0.1.2

**Langue :** [English](../FEATURES.md) | Français

Catalogue des fonctionnalités **livrées** sur l'hôte Windows/Linux (NVIDIA).
Ce n'est **pas** l'OS bootable. Les exigences v1 sont dans
[specs-fonctionnelles.md](specs-fonctionnelles.md) ; les gates dans
[STATUS.md](STATUS.md).

> Date : 15/08/2026 · Preview **0.1.2**

---

## 1. Premier lancement

| Fonctionnalité | Rôle |
|----------------|------|
| Contrôles NVIDIA + disque | `aos-session` refuse de démarrer sans GPU / espace suffisant |
| Sonde matériel | Écrit `var/run/hardware.json` (VRAM, RAM, disque) |
| Choix des modèles | Pack auto-best selon le tier VRAM (low / mid / high) |
| Téléchargement GGUF | Offerings `share/models/catalog-offerings.json` → `share/models/` |
| Tutoriel in-app | Onboarding 4 étapes (langue, confiance, routage, scénarios) |
| Boot ordonné | bus → capkd → auditd → modeld → platformd → agentd → egui |

Tiers :

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

Skills livrées : **notes-writer**, **research**, **file-author**, **planner**.

### Panneau de transparence (F-UI-04 / F-UI-05)

**Détail** agent (onglet Agents ou carte chat) :

- État live, tour *n/max*, tokens, durée, badge simple/complex
- Skills et serveurs MCP ; parent / enfants
- **Sources** (web / document / fetch) avec liens navigateur
- Timeline (`agent.trace`) : action, args, résultat, type d'outil (native / module / mcp / runtime)
- Contrôles : **Pause**, **Reprendre**, **Relancer**, **Kill**, **Steer**

---

## 6. Modèles

- Backends locaux unifiés (llama.cpp CUDA) + remote OpenAI-compatible optionnel (P3)
- Routage : **local_only** (défaut) ou **balanced** (Settings)
- Onglet Models : lister / charger / télécharger ; défaut de session
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
| Général | Langue **en** / **fr** ; trust **low** / **medium** |
| Modèles | Routage `local_only` / `balanced` |
| Réseau | Autoriser le réseau (même interrupteur que la barre latérale) |
| Agents | Modèle par défaut, max steps (1–128), timeout (60–86400 s) |
| Web | Moteur de recherche, max caractères browse, max octets fetch |

---

## 9. Audit, politique, retours, mises à jour

| Domaine | Contenu |
|---------|---------|
| Audit | Journal append-only hashé ; onglet Audit ; tuer `aos-auditd` → le superviseur le relance |
| Confirmation | Bandeau bloquant pour les actions sensibles ; timeout = refus (fail-closed) |
| Retour | Copie locale `var/feedback/` + issue GitHub optionnelle (security reste local) |
| Updates | Bandeau si une Release plus récente existe ; overlay `bin/` + `share/` sans toucher `var/` ni écraser `etc/*.yaml` |
| Site | [azerothl.github.io/akasha-os](https://azerothl.github.io/akasha-os/?lang=fr) (EN/FR) |

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

## 11. Hors Preview 0.1.2

- Image bootable / fer nu
- macOS ou inférence CPU-only
- Application automatique complète des updates (téléchargement maintenant, apply au prochain lancement)
- Génération audio / vidéo native
- Marketplace public de modules
- Comptes multi-utilisateur simultanés
- Pipeline multi-GPU complet (P5.2 ; hôte mono-GPU)

Protocole cohorte : [TESTER.md](TESTER.md).
