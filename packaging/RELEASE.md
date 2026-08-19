# GitHub Release — Akasha OS Preview

**Language:** English | [Français](../docs/fr/packaging-RELEASE.md)

## Automatic (recommended)

Tag then push:

```bash
git tag v0.9.0
git push origin v0.9.0
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
Akasha OS Preview 0.9.0 — mid-token migrate, Image studio, closed media options

- model.migrate: CPU↔GPU without aborting the live stream (NVIDIA pin cpu stays on CUDA binary)
- Closed media.* options (deny_unknown_fields); Flux2/Ideogram4/Piper extra packs
- Image studio tab + Open in studio on chat PNGs; /speak opens an in-chat TTS card
- Settings defaults for image pack / Piper voice honored after restart

Not a bootable OS. See FIRST-RUN.md / INSTALL.md / TESTER.md
```
