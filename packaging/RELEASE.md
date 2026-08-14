# GitHub Release — Agent OS Preview

**Language:** English | [Français](../docs/fr/packaging-RELEASE.md)

## Automatic (recommended)

Tag then push:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The workflow [`.github/workflows/preview-release.yml`](../.github/workflows/preview-release.yml)
builds Win + Linux (CUDA 12.4, no GGUF) and publishes:

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `latest.json` (sha256 + metadata)

Manual trigger: Actions → **preview-release** → Run workflow.

## Manual

| Asset | Command |
|-------|---------|
| Windows | `.\packaging\build-preview.ps1 -SkipModels -RequireCuda` then Compress-Archive |
| Linux | `SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh` then `tar czf` |

GGUFs are downloaded on **first run** via `share/models/manifest.json`.

## Release notes (draft)

```
Agent OS Preview — tester cohort

- Win/Linux install + NVIDIA (not a bootable OS)
- First run: model download + in-app tutorial
- Non-destructive updates via GitHub Releases
- Feedback → GitHub issues

Requirements: nvidia-smi OK, ~4 GB disk.
See FIRST-RUN.md / INSTALL.md / TESTER.md
```
