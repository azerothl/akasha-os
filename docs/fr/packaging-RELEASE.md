# GitHub Release — Akasha OS Preview

**Langue :** [English](../../packaging/RELEASE.md) | Français

## Automatique (recommandé)

Tag puis push :

```bash
git tag v0.16.1
git push origin v0.16.1
```

Le workflow [`.github/workflows/preview-release.yml`](../../.github/workflows/preview-release.yml)
construit Win + Linux **unifiés** (CUDA `aos-modeld` + CPU `aos-modeld-cpu`
dans le même zip ; sans GGUF), publie :

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `AgentOS-Preview-<ver>-macos-arm64.zip` (Apple Silicon + Metal ; CI non signé)
- `latest.json` (sha256 + métadonnées pour les artefacts testeur)

`-CpuOnly` reste un hatch **builder**, pas un téléchargement testeur.

Déclenchement manuel : Actions → **preview-release** → Run workflow.

- **macOS seul sur une Release existante** (sans retag) : `macos_only` = true, `release_version` = `0.16.1`, `upload_release` = true — attache le zip Apple Silicon et met à jour `latest.json` sur cette Release.
- **Rebuild complet** : `create_release` = true, `macos_only` = false.

## Manuel

| Asset | Commande |
|-------|----------|
| Windows GPU | `.\packaging\build-preview.ps1 -SkipModels -RequireCuda` puis Compress-Archive |
| Windows CPU | `.\packaging\build-preview.ps1 -SkipModels -CpuOnly` puis Compress-Archive |
| Linux GPU | `SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh` puis `tar czf` |
| Linux CPU | `CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh` puis `tar czf` |
| macOS Apple Silicon | `SKIP_MODELS=1 ./packaging/build-preview-macos.sh` puis `zip -r` (voir [packaging-MACOS-BUILD.md](../../packaging/MACOS-BUILD.md)) |

Les GGUF sont téléchargés au **premier run** via `share/models/manifest.json`.

## Notes de version (brouillon)

```
Akasha OS Preview 0.16.1 — Shell calme + navigation des sessions

- Chat : recherche, groupes de dates, épingle, archivage réversible
- Compositeur : multiligne Entrée/Maj+Entrée ; densité confortable par défaut
- Mode focus Canvas ; volet Activité et centre de notifications
- Sessions : un rechargement en arrière-plan n’enlève plus le chat actif
- Mêmes bandes mill Win/Linux/Mac qu'en 0.16.x

Pas un OS bootable. Voir FIRST-RUN.md / INSTALL.md / TESTER.md
```
