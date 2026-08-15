# Installation — Akasha OS Preview

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
3. Launch **Akasha OS Preview**.

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
share/skills/   Preview skills (notes-writer, research, file-author, planner)
share/mcp/      servers.yaml.example (stdio MCP)
data/models/    catalog.yaml
VERSION         build semver
FIRST-RUN.md    text tutorial
var/            local data (created at run; agents, mcp, skills overrides)
```

## First launch

1. `aos-session` checks NVIDIA + disk space and probes VRAM.
2. **Model setup** (first run): confirm auto-best pack for your GPU tier,
   then download GGUFs into `share/models/` (`catalog-offerings.json`).
3. Starts bus → capkd → auditd → modeld → platformd → agentd.
4. Opens egui UI + multi-page **tutorial**.
5. Closing the UI stops the daemons.

See [docs/FIRST-RUN.md](docs/FIRST-RUN.md) and [docs/FEATURES.md](docs/FEATURES.md).
Use the **Models** tab to download additional profiles or switch session/agent
models.

## Updates

A banner appears in the UI when a newer Release exists.
**Download** writes the archive under `var/updates/`; the **next** launch
applies `bin/` + `share/` without touching `var/` or overwriting
`etc/*.yaml` (`.new` files if needed).

## Network & search (optional)

Network is **off** by default (`offline_strict`). Enable **Allow network**
for `web.search` / `web.browse` / `net.fetch`. Search engine (`auto` =
Brave → DuckDuckGo → Bing) is set in **Settings**.

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

## Build from source

Prefer a [GitHub Release](https://github.com/azerothl/akasha-os/releases) if you
only want to run Preview. Build from source when you need a local package,
patches, or to develop against the tree.

### Toolchain

| | |
|--|--|
| OS | Windows 10/11 x64 **or** Linux x64 |
| GPU | NVIDIA driver (`nvidia-smi -L`) |
| Rust | Stable toolchain + target `wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`) |
| CUDA | CUDA Toolkit (nvcc) matching a recent 12.x install — required to compile `aos-llama` / `aos-modeld` |
| Build tools | CMake, Ninja; on Windows, a MSVC-compatible environment; on Linux, the usual X11/Wayland and clang/`libclang` deps (see `packaging/docker-build-linux.sh`) |

Disk: several GB for `target/` plus GGUF models on first run (same as the Release package).

### Clone and package

```bash
git clone https://github.com/azerothl/akasha-os.git
cd akasha-os
```

**Windows** (PowerShell) — builds bins, packages WASM modules, assembles
`dist/AgentOS-Preview-<ver>-windows-x64/` (GGUFs skipped; downloaded on first run):

```powershell
.\packaging\build-preview.ps1 -SkipModels -RequireCuda
```

Then either run from the dist folder (`.\bin\aos-session.exe` with
`$env:AOS_HOME` set to that folder) or copy `install.ps1` usage from the
Release docs onto the assembled tree.

**Linux:**

```bash
SKIP_MODELS=1 REQUIRE_CUDA=1 ./packaging/build-preview.sh
```

Output: `dist/AgentOS-Preview-<ver>-linux-x64/`. Optional: build inside a CUDA
devel container via `packaging/docker-build-linux.sh`.

### Dev run (without packaging)

From the repo root, after a successful `cargo build --release` of the Preview
crates (or after `build-preview.*`):

```powershell
# Windows
$env:AOS_HOME = (Resolve-Path .)
cargo run -p aos-session --release
```

```bash
# Linux
export AOS_HOME="$(pwd)"
cargo run -p aos-session --release
```

`aos-session` still expects NVIDIA and will drive model setup / the egui UI.
Share trees under `share/` in the repo are used when `AOS_HOME` points at the
checkout; a packaged `dist/` tree is closer to what testers get from Releases.

### Tests

```powershell
cargo test --workspace
```

Gates / demo helpers: `.\demo\run-demo.ps1 -Gate p4` (Windows).

### CI

GitHub Actions [`.github/workflows/preview-release.yml`](.github/workflows/preview-release.yml)
builds Win + Linux on tags `v*` (same scripts, no GGUF in the artifact).

## Licence

AGPL-3.0-only (`LICENSE`); commercial license available
(`LICENSE-COMMERCIAL.md`). Keep `NOTICE` with any redistribution.
