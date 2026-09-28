# GitHub Release — Akasha OS Preview

**Language:** English | [Français](../docs/fr/packaging-RELEASE.md)

## Automatic (recommended)

Tag then push:

```bash
git tag v0.19.0
git push origin v0.19.0
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

- **macOS only on existing Release** (no retag): `macos_only` = true, `release_version` = `0.19.0`, `upload_release` = true — attaches the Apple Silicon zip and refreshes `latest.json` on that Release.
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

## Pre-release checklist

Before tagging a Preview `v*` release:

- [ ] Version bumps (`VERSION`, workspace, site) match the tag
- [ ] `docs/FEATURES.md` / `docs/STATUS.md` (and FR mirrors) describe this release
- [ ] **Architecture map updated** — refresh
  [`docs/architecture-graph.json`](../docs/architecture-graph.json) and
  [`docs/architecture-map.html`](../docs/architecture-map.html) so nodes,
  edges, and flows match the shipping daemons/modules; set `meta.version` to
  this release. Policy: [docs/architecture.md](../docs/architecture.md).
  Skip only when the process layout is unchanged since the previous tag
  (document that in the release notes).

## Release notes (draft)

```
Akasha OS Preview 0.19.0 — aos-serverd + Dev-assistant P0

- aos-serverd: headless lifecycle (ADR 0012), watchdogs, local control, agent intake
- Opt-in OS services (systemd user / launchd / Windows logon) + aos-serverd in Preview zips
- Opt-in supervise aos-mcpd / aos-bridged; aos-session attach UI-only
- Dev-assistant P0: workspace.bind / fs.search / fs.apply_patch + module contract
- Builds on 0.18.0 (Memory V2, E22/E23, harness + aos-mcpd, Illustration Studio 0.7.23)

Not a bootable OS. See FIRST-RUN.md / INSTALL.md / TESTER.md
Tag follow-up: create `v0.19.0` after this docs/VERSION PR merges (do not tag from this PR alone).
```
