# GitHub Release — Akasha OS Preview

**Language:** English | [Français](../docs/fr/packaging-RELEASE.md)

## Automatic (recommended)

Tag then push:

```bash
git tag v0.11.0
git push origin v0.11.0
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
Akasha OS Preview 0.11.0 — E20 local decode

- E20: KV Q8_0 on GPU (F16 on CPU); Placement typed KV bytes
- Prefix cache: memory_seq_rm + warm llama_state_* (lower TTFT on turn 2 / E18 migrate)
- Prompt-lookup speculative decode on C1; batch N>1 unchanged
- Metrics: draft_accept / prefix_hit on Models
- E21: measured RAM + GPU/PCIe bandwidth in hardware.json; semantic prefix anchors

Not a bootable OS. See FIRST-RUN.md / INSTALL.md / TESTER.md
```
