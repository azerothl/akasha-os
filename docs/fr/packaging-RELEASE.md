# GitHub Release — Akasha OS Preview

**Langue :** [English](../../packaging/RELEASE.md) | Français

## Automatique (recommandé)

Tag puis push :

```bash
git tag v0.8.0
git push origin v0.8.0
```

Le workflow [`.github/workflows/preview-release.yml`](../../.github/workflows/preview-release.yml)
construit Win + Linux **unifiés** (CUDA `aos-modeld` + CPU `aos-modeld-cpu`
dans le même zip ; sans GGUF), publie :

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `latest.json` (sha256 + métadonnées pour **deux** artefacts testeur)

`-CpuOnly` reste un hatch **builder**, pas un téléchargement testeur.

Déclenchement manuel : Actions → **preview-release** → Run workflow.

## Manuel

| Asset | Commande |
|-------|----------|
| Windows GPU | `.\packaging\build-preview.ps1 -SkipModels -RequireCuda` puis Compress-Archive |
| Windows CPU | `.\packaging\build-preview.ps1 -SkipModels -CpuOnly` puis Compress-Archive |
| Linux GPU | `SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh` puis `tar czf` |
| Linux CPU | `CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh` puis `tar czf` |

Les GGUF sont téléchargés au **premier run** via `share/models/manifest.json`.

## Notes de version (brouillon)

```
Akasha OS Preview 0.8.0 — image + TTS locaux, artefact CPU/GPU unifié

- media.image.generate / media.audio.generate (PNG/WAV sous /downloads) ; cap media.generate
- Packs optionnels (SD 1.5, Piper) hors zip ; Download tire aussi sd.cpp / piper dans bin/
- Un zip Win / un tar Linux : aos-modeld (CUDA) + aos-modeld-cpu ; Réglages gpu/cpu/auto redémarre modeld dans la session
- Onglet Providers (cloud OpenAI-compat + loopback) ; désinstall de modules ; widgets E15 plus riches
- One-liner : irm …/install.ps1 | iex  /  curl …/install.sh | sh (sha256 fail-closed + overlay)

Pas un OS bootable. Voir FIRST-RUN.md / INSTALL.md / TESTER.md
```
