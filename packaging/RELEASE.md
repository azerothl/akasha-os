# GitHub Release — Akasha OS Preview

**Language:** English | [Français](../docs/fr/packaging-RELEASE.md)

## Automatic (recommended)

Tag then push:

```bash
git tag v0.5.0
git push origin v0.5.0
```

The workflow [`.github/workflows/preview-release.yml`](../.github/workflows/preview-release.yml)
builds Win + Linux **CUDA** and **CPU** (no GGUF) and publishes:

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-windows-x64-cpu.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `AgentOS-Preview-<ver>-linux-x64-cpu.tar.gz`
- `latest.json` (sha256 + metadata for **all four** artefacts)

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
Akasha OS Preview 0.5.0 — auto-remember from chat (E14) + user.ask

- Opt-in Settings: after each chat turn, extract durable facts → Memory
- mem.extract (local_only, low priority) + secret filter (never auto-store keys)
- Auto-link updates/supersedes; Memory [chat] badge; toast on store
- Non-blocking / coalesced extract
- user.ask: agents can pause mid-task and question the user in chat (10 min timeout)

Not a bootable OS. See FIRST-RUN.md / INSTALL.md / TESTER.md
```
