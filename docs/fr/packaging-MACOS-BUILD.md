# macOS Apple Silicon Preview — build & release

**Langue :** [English](../../packaging/MACOS-BUILD.md) | Français

Apple Silicon uniquement (`arm64`). Mac Intel **non** supporté.

## Build local (maintainer)

Sur un Mac **Apple Silicon** avec outils Xcode CLI et Rust stable :

```bash
git clone https://github.com/azerothl/akasha-os.git
cd akasha-os
rustup target add wasm32-unknown-unknown
chmod +x packaging/build-preview-macos.sh packaging/install-macos.sh
SKIP_MODELS=1 ./packaging/build-preview-macos.sh
cd dist
zip -r "AgentOS-Preview-$(tr -d '[:space:]' < ../VERSION)-macos-arm64.zip" \
  "AgentOS-Preview-$(tr -d '[:space:]' < ../VERSION)-macos-arm64"
```

Installation testeur (un geste) :

```bash
unzip AgentOS-Preview-*-macos-arm64.zip
cd AgentOS-Preview-*-macos-arm64
./install.sh
agentos-preview
```

Préfixe données : `~/.local/share/agentos-preview` (identique à Linux).

## CI

Le tag `v*` lance [`.github/workflows/preview-release.yml`](../../.github/workflows/preview-release.yml)
sur `macos-14` (Apple Silicon) et publie :

- `AgentOS-Preview-<ver>-macos-arm64.zip`

Artefact unifié : `aos-modeld` **Metal** + `aos-modeld-cpu` CPU (même logique session que Win/Linux).

## Codesign / notarisation (builds CI non signés)

Les builds GitHub Actions sont **non signés**. `install.sh` exécute `xattr -cr` sur `bin/`.

Pour une distribution plus fluide, Loïc doit exécuter **une fois** sur un Mac avec Developer ID — voir la section correspondante dans [packaging/MACOS-BUILD.md](../../packaging/MACOS-BUILD.md).

## Contraintes produit

- Pas de copy Mac sur le site mill, le chrome chat ou Settings avant un zip réel sur GitHub Releases.
- NVIDIA/CPU reste le message mill jusqu’à publication du zip Mac.
