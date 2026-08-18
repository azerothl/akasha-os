# GitHub Release — Akasha OS Preview

**Language:** English | [Français](../docs/fr/packaging-RELEASE.md)

## Automatic (recommended)

Tag then push:

```bash
git tag v0.8.0
git push origin v0.8.0
```

The workflow [`.github/workflows/preview-release.yml`](../.github/workflows/preview-release.yml)
builds Win + Linux **unified** artefacts (CUDA-linked `aos-modeld` plus
CPU-linked `aos-modeld-cpu` in the same zip; no GGUF) and publishes:

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `latest.json` (sha256 + metadata for **two** tester artefacts)

`-CpuOnly` remains a **builder** hatch, not a tester download.

Manual trigger: Actions → **preview-release** → Run workflow.

## Manual

| Asset | Command |
|-------|---------|
| Windows GPU | `.\packaging\build-preview.ps1 -SkipModels -RequireCuda` then Compress-Archive |
| Windows CPU | `.\packaging\build-preview.ps1 -SkipModels -CpuOnly` then Compress-Archive |
| Linux GPU | `SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh` then `tar czf` |
| Linux CPU | `CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh` then `tar czf` |

GGUFs are downloaded on **first run** via `share/models/manifest.json`.

## Release notes (draft)

```
Akasha OS Preview 0.8.0 — local image + TTS, unified CPU/GPU artefact

- media.image.generate / media.audio.generate (PNG/WAV under /downloads); cap media.generate
- Optional packs (SD 1.5, Piper) not in the zip; Download also fetches sd.cpp / piper into bin/
- One Win zip / one Linux tarball: aos-modeld (CUDA) + aos-modeld-cpu; Settings gpu/cpu/auto restarts modeld in-session
- Providers tab (OpenAI-compat cloud + loopback); module uninstall; richer E15 widgets
- One-liner: irm …/install.ps1 | iex  /  curl …/install.sh | sh (sha256 fail-closed + overlay)

Not a bootable OS. See FIRST-RUN.md / INSTALL.md / TESTER.md
```
