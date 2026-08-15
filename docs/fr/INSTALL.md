# Installation — Akasha OS Preview

**Langue :** [English](../../INSTALL.md) | Français

**Ce n'est pas un OS bootable.** Preview tourne **sur Windows ou Linux x64**
avec GPU **NVIDIA** (échafaudage hôte, ADR 0001). seL4 = piste séparée.

## Prérequis

| | |
|--|--|
| OS | Windows 10/11 x64 **ou** Linux x64 (glibc récent) |
| GPU | NVIDIA avec driver récent (`nvidia-smi -L` OK) |
| Disque | ~8 Go libre (pack mid recommandé) |
| CUDA | Runtime embarqué dans le paquet (driver suffit) |

Pas de macOS, pas de mode CPU-only en 0.1.

## Windows

1. Télécharger `AgentOS-Preview-<ver>-windows-x64.zip` depuis
   [GitHub Releases](https://github.com/azerothl/akasha-os/releases).
2. Décompresser, puis :
   ```powershell
   .\install.ps1
   ```
   Installe sous `%LOCALAPPDATA%\AgentOS-Preview` (préserve `var/` / `etc/`
   en cas de mise à jour).
3. Lancer **Akasha OS Preview**.

Sans installateur :
```powershell
$env:AOS_HOME = (Resolve-Path .)
.\bin\aos-session.exe
```

## Linux

1. Télécharger `AgentOS-Preview-<ver>-linux-x64.tar.gz`, extraire.
2. ```bash
   ./install.sh
   ```
   Préfixe : `~/.local/share/agentos-preview` (overlay non destructif).
3. Lancer `agentos-preview`.

## Contenu du paquet

```
bin/            daemons + CUDA runtime
share/models/   manifest.json (GGUF téléchargés au 1er run)
share/modules/  notes.aospkg, ext-rt.aospkg
share/skills/   skills Preview (notes-writer, research, file-author, planner)
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
**Télécharger** écrit l'archive dans `var/updates/` ; le **prochain**
lancement applique `bin/` + `share/` sans toucher à `var/` ni écraser
`etc/*.yaml` (fichiers `.new` si besoin).

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
| GPU NVIDIA requis | Driver NVIDIA ; `nvidia-smi -L` |
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
