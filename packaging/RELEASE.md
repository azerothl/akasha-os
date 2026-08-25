# GitHub Release — Akasha OS Preview

**Language:** English | [Français](../docs/fr/packaging-RELEASE.md)

## Automatic (recommended)

Tag then push:

```bash
git tag v0.12.0
git push origin v0.12.0
```

The workflow [`.github/workflows/preview-release.yml`](../.github/workflows/preview-release.yml)
builds Win + Linux **unified** artefacts (CUDA-linked `aos-modeld` plus
CPU-linked `aos-modeld-cpu` in the same zip; no GGUF) and publishes:

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `AgentOS-Preview-<ver>-macos-arm64.zip` (Apple Silicon + Metal; CI unsigned)
- `latest.json` (sha256 + metadata for tester artefacts)

`-CpuOnly` remains a **builder** hatch, not a tester download.

Manual trigger: Actions → **preview-release** → Run workflow.

- **macOS only on existing Release** (no retag): `macos_only` = true, `release_version` = `0.12.0`, `upload_release` = true — attaches the Apple Silicon zip and refreshes `latest.json` on that Release.
- **Full rebuild**: `create_release` = true, `macos_only` = false.

### Internal seL4 gate (not a tester release)

Tags matching `sel4-pv-*` (e.g. `sel4-pv-0.10.0`) trigger
[`.github/workflows/sel4-vm-gate.yml`](../.github/workflows/sel4-vm-gate.yml)
only. They must **not** use the `v*` prefix (that would publish Preview zips).
Artefacts: QEMU `loader.img` + serial log — no `latest.json`.

## Manual

| Asset | Command |
|-------|---------|
| Windows GPU | `.\packaging\build-preview.ps1 -SkipModels -RequireCuda` then Compress-Archive |
| Windows CPU | `.\packaging\build-preview.ps1 -SkipModels -CpuOnly` then Compress-Archive |
| Linux GPU | `SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh` then `tar czf` |
| Linux CPU | `CPU_ONLY=1 SKIP_MODELS=1 ./packaging/build-preview.sh` then `tar czf` |
| macOS Apple Silicon | `SKIP_MODELS=1 ./packaging/build-preview-macos.sh` then `zip -r` (see [MACOS-BUILD.md](MACOS-BUILD.md)) |

GGUFs are downloaded on **first run** via `share/models/manifest.json`.

## Release notes (draft)

```
```
Akasha OS Preview 0.12.0 — vision chat + macOS Apple Silicon

- Vision: InferRequest.images, catalog mmproj sidecars, UI image attach (gated on non-vision models)
- Canvas: agent tools, set_style live stroke colors, draw fan-out fixes
- macOS: Apple Silicon Preview zip (Metal + CPU modeld); unsigned — Gatekeeper on install.sh

Not a bootable OS. See FIRST-RUN.md / INSTALL.md / TESTER.md
```
