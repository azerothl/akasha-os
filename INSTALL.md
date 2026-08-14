# Installation — Agent OS Preview

**Language:** English | [Français](docs/fr/INSTALL.md)

**This is not a bootable OS.** Preview runs on **Windows or Linux x64** with an
**NVIDIA** GPU (host scaffolding, ADR 0001). seL4 is a separate track.

## Requirements

| | |
|--|--|
| OS | Windows 10/11 x64 **or** Linux x64 (recent glibc) |
| GPU | NVIDIA with a recent driver (`nvidia-smi -L` OK) |
| Disk | ~8 GB free (recommended mid-tier GGUF pack) |
| CUDA | Runtime shipped in the package (driver is enough) |

No macOS, no CPU-only mode in 0.1.

## Windows

1. Download `AgentOS-Preview-<ver>-windows-x64.zip` from
   [GitHub Releases](https://github.com/azerothl/akasha-os/releases).
2. Extract, then:
   ```powershell
   .\install.ps1
   ```
   Installs under `%LOCALAPPDATA%\AgentOS-Preview` (preserves `var/` / `etc/`
   on update).
3. Launch **Agent OS Preview**.

Without the installer:
```powershell
$env:AOS_HOME = (Resolve-Path .)
.\bin\aos-session.exe
```

## Linux

1. Download `AgentOS-Preview-<ver>-linux-x64.tar.gz`, extract.
2. ```bash
   ./install.sh
   ```
   Prefix: `~/.local/share/agentos-preview` (non-destructive overlay).
3. Run `agentos-preview`.

## Package contents

```
bin/            daemons + CUDA runtime
share/models/   manifest.json (GGUF downloaded on first run)
share/modules/  notes.aospkg, ext-rt.aospkg
share/skills/   Preview skills
data/models/    catalog.yaml
VERSION         build semver
FIRST-RUN.md    text tutorial
var/            local data (created at run)
```

## First launch

1. `aos-session` checks NVIDIA + disk space and probes VRAM.
2. **Model setup** (first run): confirm auto-best pack for your GPU tier,
   then download GGUFs into `share/models/` (`catalog-offerings.json`).
3. Starts bus → capkd → auditd → modeld → platformd → agentd.
4. Opens egui UI + multi-page **tutorial**.
5. Closing the UI stops the daemons.

See [docs/FIRST-RUN.md](docs/FIRST-RUN.md). Use the **Models** tab to download
additional profiles or switch session/agent models.

## Updates

A banner appears in the UI when a newer Release exists.
**Download** writes the archive under `var/updates/`; the **next** launch
applies `bin/` + `share/` without touching `var/` or overwriting
`etc/*.yaml` (`.new` files if needed).

## Network & search (optional)

Network is **off** by default (`offline_strict`). Enable **Allow network**
for `web.search` / `net.fetch`.

```yaml
# var/secrets/keys.yaml (optional)
keys:
  brave_search_api_key: "BSA..."
  github_token: "ghp_..."   # one-click Feedback issues
```

## Troubleshooting

| Symptom | Action |
|---------|--------|
| NVIDIA GPU required | NVIDIA driver; `nvidia-smi -L` |
| Model download failed | Network for HF, or copy GGUFs into `share/models/` |
| Healthcheck failed | `var/run/*.stderr.log` (**Troubleshooting** button) |
| Bus unreachable | Always launch via `aos-session` |

## Build / CI (maintainers)

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda
```

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
```

GitHub Actions: `.github/workflows/preview-release.yml` (tags `v*`).

## Licence

AGPL-3.0-only (`LICENSE`); commercial license available
(`LICENSE-COMMERCIAL.md`). Keep `NOTICE` with any redistribution.
