# Phase P08 — Preview 0.8.0

**Langue :** [English](../../phases/phase-preview-08.md) | Français

## Objectif

Livrer **Akasha OS Preview 0.8.0** : **génération d’image + audio (TTS)**
(E16) ; un **artefact hôte CPU/GPU unifié** avec politique device UI + charge
(E17) ; **désinstallation complète d’un module** depuis l’UI testeur
(F-MOD-01) ; un **élargissement du vocabulaire E15** pour que les modules
créés aient des dashboards plus riches sans webview ; un onglet **Providers**
pour ajouter des serveurs OpenAI-compatibles cloud et locaux (F-MDL-04) ;
**une commande d’install par OS** ; plus une **passe de
nettoyage / refacto** avant le tag. Même
bus, caps, audit et Placement Manager que le chat GGUF. Pas seL4 / fer nu.
Pas de vidéo. Pas de voix always-on. Pas un sidecar cloud-only. Pas de
nouveau numéro de gate P6.

Priorités : [plan-evolutions.md](../plan-evolutions.md) Horizon B (E16, E17)
plus suivi E15 et F-MDL-04. Séquençage : P08.1 → P08.2 ∥ P08.3 ∥ P08.5 ∥
P08.7 ∥ P08.11 ∥ P08.12 → P08.4 → P08.6 → P08.8 → P08.9 → P08.10.

## Pourquoi ça, pas une API image hébergée

egui reste le shell (ADR 0003). Diffusion et TTS **se disputent la VRAM**
avec le LLM chargé (F-PLC-06 / F-PLC-09). Les agents ont besoin d’une
capacité explicite `media.generate` ; les sorties atterrissent sous
`/downloads` avec une piste d’audit. Un backend distant compatible OpenAI
pour l’image pourra exister plus tard comme backend *routé* (`local_only`
gagne toujours pour les données `secret`) — ce n’est pas le chemin par
défaut en 0.8.

STT / voix 24/7 / canaux de chat restent au sibling (anti-roadmap).

## Pourquoi unifier CPU et GPU (E17)

Aujourd’hui le testeur télécharge un **zip CUDA ou un zip CPU**. Settings
expose déjà Inférence **auto / gpu / cpu**, mais ça ne s’applique qu’au
**prochain boot complet**, et `auto` veut dire « NVIDIA présent ? », pas la
charge. Un `aos-modeld` lié CUDA peut refuser de démarrer sans les DLL
NVIDIA — d’où le split.

Preview 0.8 livre **un artefact par OS**. `aos-session` sonde le hardware et
lance un backend sûr sur cette machine (le process CPU n’a pas de dépendance
DLL CUDA). Settings **gpu / cpu** redémarre `aos-modeld` dans la session
courante (annuler l’inférence en cours d’abord ; **la migration mid-token
sans cancel est P09 / E18**). **auto** est une politique
du Placement Manager avec hystérésis : GPU tant que la VRAM le permet ;
plus d’offload RAM/CPU (ou cpu-only) sous pression VRAM/CPU ou quand E16 a
besoin du GPU ; promotion inverse quand la pression retombe. Un pin
`gpu` / `cpu` surcharge auto.

`-CpuOnly` reste une échappatoire **builder** (pas de toolkit CUDA sur la
machine de build), pas un second téléchargement testeur.

## Livrables

| # | Évolution | Livrable | État |
|---|-----------|----------|------|
| P08.1 | E16 registre | Image + TTS dans le catalogue / offerings ; le Placement Manager évince LLM vs shards média | fait |
| P08.2 | E16 image | `media.image.generate` (prompt → PNG sous `/downloads`) ; revue de caps ; audit | fait |
| P08.3 | E16 audio | `media.audio.generate` (texte → WAV/OGG TTS) ; même famille de caps ; audit | fait |
| P08.4 | E16 surface | Le chat montre l’image / joue le clip (kinds E15 `image` / `audio` en P08.11) | fait |
| P08.5 | E17 device | Un artefact Win + un Linux ; Settings gpu/cpu/auto sans réinstall ; auto suit la charge VRAM/CPU (hystérésis) | fait |
| P08.6 | E16 packs | Packs média optionnels (téléchargés, pas cuits dans le zip) ; le même Download installe sd.cpp / piper dans `bin/` s’il manque ; GPU préféré pour l’image | fait |
| P08.7 | F-MOD-01 | Désinstaller tout module non bundlé depuis l’UI ; révoquer les caps ; retirer l’onglet E15 ; audit | fait |
| P08.8 | Hygiène | Nettoyage + refacto des crates hôte Preview (code mort, découpes, nommage) ; **pas** de changement de comportement | fait |
| P08.9 | Install CLI | Une commande documentée par OS : download + sha256 + overlay dans le préfixe stable | fait |
| P08.10 | Docs / ship | Docs de phase, FEATURES/STATUS/TESTER, version 0.8.0, site, packaging | fait |
| P08.11 | E15 widgets | Vocabulaire fermé étendu : JSON Schema des `form` + `select` / `radio` / `checkbox` / `textarea` / `bar_chart` / `image` / `audio` | fait |
| P08.12 | F-MDL-04 | Onglet **Providers** : ajouter / lister / tester / retirer serveurs OpenAI-compatibles cloud + locaux ; modèles sélectionnables dans le Chat | fait |

Catalogue (une fois livré) : [`docs/fr/FEATURES.md`](../FEATURES.md).

### Placement

Les modèles média sont des shards évincables, pas un second client GPU
non géré. Si la VRAM ne tient pas LLM + diffusion : décharger ou refuser
avec une alternative (pack plus petit / TTS CPU / skip explicite). Le TTS
CPU est dans le périmètre ; la génération d’image CPU peut être lente ou
sautée avec un refus documenté.

### Désinstallation de module (P08.7 / F-MOD-01)

`module.uninstall` existe déjà sur le bus et un bouton Settings catalogue
l’appelle, mais ce n’est pas un chemin testeur complet : seulement les
lignes du catalogue, pas de révocation de caps (exigée par la spec), pas de
confirm, et les modules créés par un agent (E15) passent facilement à côté.

0.8 livre la désinstallation comme surface humaine de première classe :

- Lister **tous** les modules installés (pas seulement le catalogue signé)
- Confirmer, puis retirer le répertoire + registre ; **révoquer** les caps
  `tool.invoke:<name>` (et liées) accordées ; auditer la chaîne
- L’onglet E15 et le panneau en mémoire disparaissent tout de suite
- Les bundlés `notes` / `tasks` / `ext-rt` sont **refusés** (le resync au
  boot les rétablirait)
- Ne **supprime pas** les documents utilisateur du module sous `/documents`
- Un agent a besoin de `module.uninstall` + le même style de confirmation
  que l’install

Étape TESTER : installer un module scaffoldé → désinstaller → onglet parti,
caps parties, réinstall encore possible.

### Vocabulaire E15 (P08.11)

L’arbre fermé de 0.7 (`column`, `row`, `heading`, `text`, `markdown`,
`stat_row`, `table`, `line_chart`, `form`, `button`) est trop mince pour
de vrais dashboards : `form` n’est que du texte, pas de select/radio, une
seule courbe. Un auteur ne peut pas inventer un `kind` — l’inconnu reste
**fail-closed**. 0.8 élargit la liste **côté hôte**, toujours fermée.

Must :

- **`form` honore le JSON Schema** de l’outil lié : `string` → texte,
  `integer`/`number` → champ numérique, `boolean` → checkbox, `enum` →
  ComboBox ; `format: textarea` (ou chaîne longue) → multiligne
- Nouveaux kinds : `select`, `radio`, `checkbox`, `textarea`
- Graphiques : `bar_chart` (en plus de `line_chart`)
- Média (partagé avec P08.4) : `image`, `audio` — bind vers un chemin ou
  un résultat d’outil sous `/downloads`

Toujours fail-closed : kinds inconnus refusés, pas d’arbre partiel, export
JSON Schema à jour (`docs/bridge/aos-proto-decl-ui.json`). L’arbre scaffold
par défaut peut utiliser un select + table si l’outil primaire a un `enum`.

Hors pack : camembert/scatter/canvas, date/couleur, cartes, éditeur riche,
réécriture Notes/Tasks sur E15, `sandboxed_webview`, plugins de kinds
tiers.

Étape TESTER : scaffolder/installer un module script dont l’UI utilise
`select` (ou un champ `enum`), `checkbox` ou `radio`, et `bar_chart` ;
soumettre une fois ; le résultat se rafraîchit. Un `kind` invalide affiche
toujours une bannière d’erreur.

### Onglet Providers (P08.12 / F-MDL-04)

P3 a déjà `model.backend.add` et un seul champ Settings « clé OpenAI /
remote ». Le testeur ne peut pas gérer plusieurs endpoints, découvrir les
modèles, ni pointer un serveur local OpenAI-compatible (Ollama / vLLM /
LM Studio) sans éditer du YAML. 0.8 livre un onglet **Providers** de
première classe.

Must :

- CRUD : ajouter, éditer, activer/désactiver, retirer ; persister sous
  `var/providers/` (ou équivalent) ; recharger au boot
- Protocole : **OpenAI-compatible** `/v1/chat/completions` + découverte
  optionnelle `GET /v1/models` (même client que P3.1)
- Presets (URL de base + nom de secret, surchargeable) :

  | Preset | Endpoint par défaut | Secret |
  |--------|---------------------|--------|
  | OpenAI | `https://api.openai.com/v1` | vault |
  | OpenRouter | `https://openrouter.ai/api/v1` | vault |
  | Anthropic | base OpenAI-compat (pas l’API Messages native) | vault |
  | DeepSeek | `https://api.deepseek.com/v1` | vault |
  | z.ai | base OpenAI-compat du vendor | vault |
  | Custom | URL utilisateur | vault optionnel |
  | Ollama | `http://127.0.0.1:11434/v1` | aucun |
  | vLLM | `http://127.0.0.1:8000/v1` | optionnel |
  | LM Studio | `http://127.0.0.1:1234/v1` | aucun |

- Bouton **Test** : connectivité + liste de modèles (ou un id saisi si la
  découverte est vide)
- Le combo Chat / Models peut sélectionner un modèle de provider si le
  routage le permet
- Les clés restent dans le vault (jamais dans le fichier provider, jamais
  aux agents)
- `local_only` (défaut) ignore les providers cloud ; `balanced` /
  `remote_only` peuvent les utiliser ; les données **`secret` ne sortent
  jamais** (règle P3)
- Loopback (`127.0.0.1` / `localhost`) = **privacy locale** et **n’exige
  pas** « Autoriser le réseau » ; les providers WAN si
- Hors-ligne / pas de clé / injoignable → erreur claire, le GGUF local
  continue
- Audit : add / test / infer-via-provider

Hors pack : protocoles natifs Anthropic Messages / Gemini / Bedrock,
marketplace de providers, faire du remote le défaut, livrer des clés API.

Étape TESTER : ajouter LM Studio ou Ollama en loopback avec `local_only`
toujours on → le chat l’utilise ; ajouter un preset cloud avec clé vault →
refusé tant que Allow network + `balanced` ; un tour `secret` reste local.

### Install en une ligne par OS (P08.9)

Aujourd’hui le testeur doit trouver le bon zip GitHub Release, extraire,
puis lancer `install.cmd` / `install.ps1` ou `./install.sh` (et se battre
avec `ExecutionPolicy` Windows sur `.\install.ps1`). 0.8 publie **une
commande à coller par OS** sur INSTALL, le site et TESTER.

Surface visée (URLs figées en P08.10) :

```powershell
# Windows (PowerShell) — Bypass au scope Process, pas de fichier local non signé
irm https://azerothl.github.io/akasha-os/install.ps1 | iex
```

```bash
# Linux x64
curl -fsSL https://azerothl.github.io/akasha-os/install.sh | sh
```

Le script hébergé : lit `latest.json` des GitHub Releases, prend l’artefact
Win ou Linux **unifié** (E17), vérifie le sha256, extrait, lance l’overlay
non destructif vers `%LOCALAPPDATA%\AgentOS-Preview` ou
`~/.local/share/agentos-preview`, affiche la commande de lancement. HTTPS
uniquement ; afficher URL + hash avant d’écrire. Fail-closed si le hash
ne matche pas. Pas de `cargo`.

Pas winget / apt / Chocolatey. Pas macOS. Authenticode/SmartScreen reste
un sujet certificat éditeur plus tard (INSTALL documente déjà **Plus
d’infos → Exécuter quand même**).

### Nettoyage / refacto (P08.8)

Passe dédiée **après** E16/E17/désinstall/P08.11/P08.12 et **avant** le tag, pour inclure
le nouveau code média, device, uninstall, widgets et providers. Périmètre : hôte Preview (`crates/aos-*`, egui, scripts de packaging
touchés par 0.8) — pas une réécriture seL4, pas Notes/Tasks sur E15, pas un
nouveau toolkit UI.

Dans le périmètre : code mort et deps inutilisées ; modules trop gros
découpés selon les frontières de services déjà là ; helpers dupliqués
fusionnés ; nommage intents / caps / fichiers aligné sur `media.*` ; labels
UI bilingues qui ont dérivé. Gate : préservation du comportement
(`cargo test --workspace` + p4/p5 inchangés). Commits d’hygiène séparés des
commits fonctionnels E16. Le reliquat après cette passe (`main.rs` egui /
`aos-platformd.rs` encore trop gros ; onglet Scénarios et clé de rôle chat
`"vous"`) est **P09.7**, pas abandonné.

## Gates de sortie

| Gate | Critère |
|------|---------|
| P08.1 | Le catalogue liste au moins un pack image et un pack TTS ; charger un média n’échappe pas à la comptabilité VRAM du Placement Manager |
| P08.2 | Un prompt depuis le chat ou un outil de module écrit un PNG sous `/downloads` ; audité ; agent sans `media.generate` refusé |
| P08.3 | Texte → fichier audio jouable sous `/downloads` ; mêmes règles de caps / audit que l’image |
| P08.4 | Le testeur voit l’image dans le chat et peut jouer le clip sans quitter Preview |
| P08.5 | Le même zip démarre sans NVIDIA (backend CPU) et utilise CUDA s’il est là ; Settings gpu/cpu après restart de modeld (pas de réinstall) ; auto rétrograde/promeut sous charge avec hystérésis ; le pin surcharge auto |
| P08.6 | Un hôte CUDA peut télécharger les packs média **et** leurs moteurs (`bin/sd` / `bin/piper`) ; un hôte CPU-only démarre sans eux |
| P08.7 | Désinstaller un module non bundlé depuis l’UI (confirm) ; package parti ; caps accordées révoquées (audité) ; onglet E15 parti ; `notes`/`tasks`/`ext-rt` refusent |
| P08.8 | PR(s) d’hygiène sans changement de comportement volontaire ; tests workspace + p4/p5 toujours verts |
| P08.9 | Sur une machine Win et Linux neuves, la one-liner documentée télécharge, vérifie le sha256, overlay le préfixe stable, et Preview peut démarrer (pas de chasse au zip, pas de cargo) |
| P08.10 | FEATURES/STATUS/TESTER + version 0.8.0 |
| P08.11 | L’onglet d’un module installé rend `select`/`radio`/`checkbox`/`textarea`/`bar_chart` (et `image`/`audio` s’il y a un bind) ; les champs `form` suivent les types JSON Schema ; un `kind` inconnu est toujours refusé (fail-closed, pas d’arbre partiel) |
| P08.12 | Onglet Providers : ajouter un preset loopback et un preset cloud ; les modèles apparaissent dans le Chat ; `local_only` bloque le WAN ; le loopback marche sans Allow network ; la clé vault n’est jamais écrite dans le fichier provider ; un infer `secret` reste local |
| Régression | `cargo test --workspace` ; gates p4/p5 verts sur hôte CUDA |
| Packaging | Deux artefacts testeur (Win / Linux) + `latest.json` complet + `install.ps1` / `install.sh` hébergés (`-CpuOnly` = builder seulement) |

## Hors périmètre

Génération vidéo, APIs image cloud comme chemin par défaut, micro always-on
/ STT, canaux de messagerie, computer-use, `sandboxed_webview`, compositor
E13, hot-swap device en milieu de token sans cancel (**c’est P09 / E18**), E7 TPM, daemon HTTP sibling live, E9 / P5.2 multi-GPU, marketplace
public, winget/apt/Chocolatey, fermeture cohort PC, macOS, fer nu (E11–E13),
APIs natives Anthropic Messages / Gemini / Bedrock (presets OpenAI-compat
seulement).

## Build

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda   # artefact testeur (backends GPU+CPU)
.\packaging\build-preview.ps1 -SkipModels -CpuOnly       # builder seulement, pas de toolkit CUDA
```

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh   # builder seulement
```

## Suite

Tag `v0.8.0` seulement quand les gates P08.1–P08.12 passent. La gate cohort
PC reste indépendante. **Suite : Preview 0.9.0 / E18 + E19** — migration de device
en milieu de token sans cancel, plus packs image/TTS extra et schéma d’options
fermé, plus reliquat d’hygiène P08.8 ([phase-preview-09.md](phase-preview-09.md)
P09.7).
Après 0.9 : reste Horizon B (E7 TPM, adaptateur HTTP live si un daemon est
planifié, E9 quand un 2e GPU existe).
