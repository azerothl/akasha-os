# Installation — Akasha OS Preview 0.8.0

**Langue :** [English](../INSTALL.md) | Français

> Date : 18/08/2026 · Preview **0.8.0**

**Ce n'est pas un OS bootable.** La Preview 0.8.0 tourne **sur Windows ou Linux x64**
(échafaudage hôte, ADR 0001). **NVIDIA est recommandé** ; le **même zip**
embarque `aos-modeld-cpu` (Réglages → Inférence). seL4 = piste séparée.

## Une commande

Windows :

```powershell
irm https://azerothl.github.io/akasha-os/install.ps1 | iex
```

Linux :

```bash
curl -fsSL https://azerothl.github.io/akasha-os/install.sh | sh
```

Le script lit `latest.json` des GitHub Releases, affiche URL + sha256,
**refuse** en cas d’écart, puis superpose dans
`%LOCALAPPDATA%\AgentOS-Preview` ou `~/.local/share/agentos-preview`.

## Prérequis

| | |
|--|--|
| OS | Windows 10/11 x64 **ou** Linux x64 (glibc récent) |
| GPU | NVIDIA avec driver récent (`nvidia-smi -L` OK) **ou** artefact CPU-only |
| Disque | ~8 Go libre (pack mid recommandé) ; ~4,5 Go de plus pour SD 1.5 + moteur (optionnel) |
| CUDA | Runtime embarqué dans le paquet GPU (driver suffit) ; pas sur le paquet CPU |

Pas de macOS. L'inférence CPU est supportée mais dégradée.

## Windows

1. One-liner ci-dessus, **ou** télécharger le zip unifié
   `AgentOS-Preview-<ver>-windows-x64.zip` depuis
   [GitHub Releases](https://github.com/azerothl/akasha-os/releases).
2. Si vous avez le zip : décompresser, puis lancer **l’un** de :
   ```bat
   .\install.cmd
   ```
   ou, en PowerShell explicite :
   ```powershell
   powershell -ExecutionPolicy Bypass -File .\install.ps1
   ```
   (`.\install.ps1` seul échoue si la stratégie est `Restricted` /
   `AllSigned` — scripts non signés bloqués.)
   Installe sous `%LOCALAPPDATA%\AgentOS-Preview` (préserve `var/` / `etc/`
   en cas de mise à jour).
3. Lancer **Akasha OS Preview**.

**Les données utilisateur restent dans ce préfixe stable** (sessions, mémoire,
notes, secrets, registre modèles). Extraire un nouveau zip et lancer
`bin\aos-session.exe` synchronise le programme vers le même emplacement **sans
écraser** `var/` / `etc/`. Ne pas fixer `AOS_HOME` sur le dossier versionné
sauf isolation volontaire. Mode portable : fichier `.portable` à côté de
`VERSION`, ou `AOS_PORTABLE=1`.

### SmartScreen Windows (« Éditeur inconnu »)

Les binaires Preview ne sont **pas encore signés** Authenticode. SmartScreen
peut bloquer `aos-session.exe` (ou le raccourci) avec *Windows a protégé votre
ordinateur*.

1. Cliquer **Informations complémentaires**.
2. Cliquer **Exécuter quand même**.

Attendu pour des builds GitHub non signés. Une signature éditeur (Azure Trusted
Signing / certificat EV) est le vrai correctif pour les releases suivantes —
ce n’est pas, en soi, un signal malware.

Démarrage rapide sans `install.cmd` (préfixe de données stable quand même) :
```powershell
.\bin\aos-session.exe
```

## Linux

1. One-liner ci-dessus, **ou** télécharger `…-linux-x64.tar.gz` unifié, extraire.
2. ```bash
   ./install.sh
   ```
   Préfixe : `~/.local/share/agentos-preview` (overlay non destructif).
3. Lancer `agentos-preview`.

## Contenu du paquet

```
bin/            daemons (+ runtime CUDA sur builds GPU)
share/models/   manifest.json (GGUF téléchargés au 1er run)
share/modules/  notes.aospkg, tasks.aospkg, ext-rt.aospkg
share/skills/   skills Preview (notes-writer, research, file-author, planner, tasks)
share/mcp/      servers.yaml.example (MCP stdio)
data/models/    catalog.yaml
VERSION         semver du build
FIRST-RUN.md    tutoriel texte
var/            données locales (créé au run ; agents, mcp, skills)
```

## Premier lancement

1. `aos-session` vérifie NVIDIA + disque et sonde la VRAM.
2. **Choix des modèles** (1er run) : confirmer le pack auto-best, télécharger
   les GGUF (`catalog-offerings.json`).
3. Démarre bus → capkd → auditd → modeld → platformd → agentd.
4. Ouvre l'UI egui + **tutoriel**.
5. Fermer l'UI arrête les daemons.

Voir [FIRST-RUN.md](FIRST-RUN.md) et [FEATURES.md](FEATURES.md).
Onglet **Models** pour d'autres profils.

## Mises à jour

Un bandeau apparaît dans l'UI si une Release plus récente existe.
**Télécharger** (ou Settings → **Télécharger les mises à jour automatiquement**,
opt-in) écrit l'archive dans `var/updates/` ; le **prochain** lancement
applique `bin/` + `share/` sans toucher à `var/` ni écraser `etc/*.yaml`
(fichiers `.new` si besoin). Si un téléchargement est en attente, le bandeau
indique que l'update est prête au relancement.

Lancer un zip de Release fraîchement extrait (ou `install.cmd` à nouveau)
applique le même overlay vers `%LOCALAPPDATA%\AgentOS-Preview` /
`~/.local/share/agentos-preview`. Sessions, mémoire, notes et secrets restent.

## Réseau & recherche (optionnel)

Par défaut le réseau est **coupé** (`offline_strict`). Case
**Autoriser le réseau** pour `web.search` / `web.browse` / `net.fetch`.
Le moteur (`auto` = Brave → DuckDuckGo → Bing) se règle dans **Settings**.

```yaml
# var/secrets/keys.yaml (optionnel)
keys:
  brave_search_api_key: "BSA..."
  github_token: "ghp_..."   # issues Feedback en un clic
```

## Dépannage

| Symptôme | Action |
|----------|--------|
| NVIDIA recommandé | Driver + `nvidia-smi -L`, ou paquet CPU / Settings → CPU |
| Échec modèles | Réseau pour HF, ou copier les GGUF dans `share/models/` |
| healthcheck échoué | `var/run/*.stderr.log` (bouton **Dépannage**) |
| Bus injoignable | Toujours via `aos-session` |

## Build depuis les sources

Préférez une [Release GitHub](https://github.com/azerothl/akasha-os/releases) si
vous voulez seulement lancer la Preview. Compilez depuis les sources pour un
paquet local, des patches, ou du développement sur l’arbre.

### Toolchain

| | |
|--|--|
| OS | Windows 10/11 x64 **ou** Linux x64 |
| GPU | Pilote NVIDIA (`nvidia-smi -L`) |
| Rust | Toolchain stable + cible `wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`) |
| CUDA | CUDA Toolkit (nvcc), typiquement 12.x — requis pour compiler `aos-llama` / `aos-modeld` |
| Outils | CMake, Ninja ; sous Windows, environnement compatible MSVC ; sous Linux, deps X11/Wayland et clang/`libclang` (voir `packaging/docker-build-linux.sh`) |

Disque : plusieurs Go pour `target/` plus les modèles GGUF au premier lancement
(comme le paquet Release).

### Clone et paquet

```bash
git clone https://github.com/azerothl/akasha-os.git
cd akasha-os
```

**Windows** (PowerShell) — compile les binaires, empaquete les modules WASM,
assemble `dist/AgentOS-Preview-<ver>-windows-x64/` (sans GGUF ; téléchargés au
premier run) :

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda
```

Ensuite lancez depuis le dossier dist (`.\bin\aos-session.exe` avec
`$env:AOS_HOME` pointant dessus) ou utilisez `install.ps1` comme pour une
Release.

**Linux :**

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
```

Sortie : `dist/AgentOS-Preview-<ver>-linux-x64/`. Option : build dans un
conteneur CUDA devel via `packaging/docker-build-linux.sh`.

### Run de développement (sans packaging)

Depuis la racine du dépôt, après un `cargo build --release` des crates Preview
(ou après `build-preview.*`) :

```powershell
# Windows
$env:AOS_HOME = (Resolve-Path .)
cargo run -p aos-session --release
```

```bash
# Linux
export AOS_HOME="$(pwd)"
cargo run -p aos-session --release
```

`aos-session` exige toujours NVIDIA et enchaîne le setup modèles / l’UI egui.
Avec `AOS_HOME` sur le checkout, les arbres `share/` du dépôt sont utilisés ;
un arbre `dist/` ressemble davantage au paquet testeur.

### Tests

```powershell
cargo test --workspace
```

Gates / démo : `.\demo\run-demo.ps1 -Gate p4` (Windows).

### CI

GitHub Actions [`.github/workflows/preview-release.yml`](../../.github/workflows/preview-release.yml)
construit Win + Linux sur les tags `v*` (mêmes scripts, sans GGUF dans
l’artefact).

## Licence

AGPL-3.0-only (`LICENSE`) ; licence commerciale possible
(`LICENSE-COMMERCIAL.md`). Conservez `NOTICE` avec toute redistribution.
