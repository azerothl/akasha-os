# GitHub Release — Akasha OS Preview

**Language:** English | [Français](../docs/fr/packaging-RELEASE.md)

## Automatic (recommended)

Tag then push:

```bash
git tag v0.10.0
git push origin v0.10.0
```

The workflow [`.github/workflows/preview-release.yml`](../.github/workflows/preview-release.yml)
builds Win + Linux **unified** artefacts (CUDA-linked `aos-modeld` plus
CPU-linked `aos-modeld-cpu` in the same zip; no GGUF) and publishes:

- `AgentOS-Preview-<ver>-windows-x64.zip`
- `AgentOS-Preview-<ver>-linux-x64.tar.gz`
- `latest.json` (sha256 + metadata for **two** tester artefacts)

`-CpuOnly` remains a **builder** hatch, not a tester download.

Manual trigger: Actions → **preview-release** → Run workflow.

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

GGUFs are downloaded on **first run** via `share/models/manifest.json`.

## Release notes (draft)

```
Akasha OS Preview 0.10.0 — Horizon B: TPM vault, sibling bridge, multi-GPU path

- E7 TPM: vault master prefers host TPM envelope when available (fallback keyring/file)
- E8: optional aos-bridged loopback HTTP↔bus (mem + secrets); not inside aos-session
- E9: multi-GPU tensor_split plumbing; P5 gate SKIPs on 1-GPU hosts
- Media polish: Image studio composition + upscale; Wan/LTX experimental
- Internal seL4: tag sel4-pv-0.10.0 + CI QEMU gate (not in tester zip)

Not a bootable OS. See FIRST-RUN.md / INSTALL.md / TESTER.md
```
