# GitHub Release — Agent OS Preview

**Langue :** [English](../../packaging/RELEASE.md) | Français

## Automatique (recommandé)

Tag puis push :

```bash
git tag v0.1.0
git push origin v0.1.0
```

Le workflow [`.github/workflows/preview-release.yml`](../../.github/workflows/preview-release.yml)
construit Win + Linux (CUDA 12.4, sans GGUF), publie :

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `latest.json` (sha256 + métadonnées)

Déclenchement manuel : Actions → **preview-release** → Run workflow.

## Manuel

| Asset | Commande |
|-------|----------|
| Windows | `.\packaging\build-preview.ps1 -SkipModels -RequireCuda` puis Compress-Archive |
| Linux | `SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh` puis `tar czf` |

Les GGUF sont téléchargés au **premier run** via `share/models/manifest.json`.

## Notes de version (brouillon)

```
Agent OS Preview — cohorte de test

- Install Win/Linux + NVIDIA (pas un OS bootable)
- Premier run : download modèles + tutoriel in-app
- Updates non destructives via GitHub Releases
- Retours → issues GitHub

Prérequis : nvidia-smi OK, ~4 Go disque.
Voir FIRST-RUN.md / INSTALL.md / TESTER.md
```
