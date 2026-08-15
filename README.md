# Akasha OS

**Language:** English | [Français](docs/fr/README.md)

**Site:** [azerothl.github.io/akasha-os](https://azerothl.github.io/akasha-os/)

**Agent-native operating system** — capability-based security, semantic IPC,
first-class GPU, offline-first. Preview builds run on a Windows/Linux host
(NVIDIA); a seL4 bare-metal track is separate.

> This is **not** a bootable OS image yet. Preview 0.2.0 is an installable host
> app for testers.

## Why Akasha OS

Most agent stacks are apps on top of a general-purpose OS. Akasha OS treats
agents, models, tools, and memory as **first-class system services** with
explicit capabilities, audit, and policy — so autonomy stays bounded and
observable.

## Preview features

Full catalogue: [docs/FEATURES.md](docs/FEATURES.md).

| Area | What you get |
|------|----------------|
| Chat / Sessions | Parallel persisted conversations; slash commands; per-session model |
| Memory | Long-term facts; memory-first agent bootstrap |
| Notes | Human + agent-authored notes (WASM); resyncs on boot after an update |
| Agents | Goal loop, skills, tools, MCP; transparency timeline; steer / pause |
| Models | Hardware-aware first-run packs; download / load; local-only by default |
| Network | Opt-in search (Brave / DDG / Bing) + `web.browse` + fetch |
| Settings | Language, trust, routing, agent defaults, search engine |
| Feedback | Local report + GitHub issue on this repo |
| Troubleshoot | In-app diagnostics; GitHub report when findings exist |
| Updates | Non-destructive overlays from GitHub Releases |

## Requirements

- Windows 10/11 x64 **or** Linux x64
- NVIDIA GPU + recent driver (`nvidia-smi -L`)
- ~4 GB free disk (binaries + GGUF models downloaded on first run)

No macOS / CPU-only mode in Preview 0.2.0.

## Quick start

1. Download the latest zip/tar.gz from
   [GitHub Releases](https://github.com/azerothl/akasha-os/releases).
2. Run `install.ps1` (Windows) or `./install.sh` (Linux).
3. Launch **Akasha OS Preview** — first run downloads models if needed, then
   opens the in-app tutorial.

See [INSTALL.md](INSTALL.md) and [docs/FIRST-RUN.md](docs/FIRST-RUN.md).
Cohort protocol: [docs/TESTER.md](docs/TESTER.md).

### Developers

```powershell
cargo test --workspace
$env:AOS_HOME = (Resolve-Path .)
cargo run -p aos-session --release
.\demo\run-demo.ps1 -Gate p4
```

## Documentation

| Doc | Description |
|-----|-------------|
| [docs/STATUS.md](docs/STATUS.md) | **Project status** (phases & gates) |
| [docs/FEATURES.md](docs/FEATURES.md) | **Shipped Preview features** |
| [docs/FIRST-RUN.md](docs/FIRST-RUN.md) | First-run guide |
| [docs/functional-specs.md](docs/functional-specs.md) | Functional requirements |
| [docs/technical-specs.md](docs/technical-specs.md) | Architecture & APIs |
| [docs/development-plan.md](docs/development-plan.md) | Phase plan |
| [docs/vision.md](docs/vision.md) | Product vision |
| [docs/I18N.md](docs/I18N.md) | Language layout (EN / FR) |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution license terms |

French mirrors live under [`docs/fr/`](docs/fr/).

## Licence

**Dual licensing.**

- **AGPL-3.0-only** ([`LICENSE`](LICENSE)) — free use/modify/distribute,
  including commercially, if you honor AGPL (source available, including
  network use). Keep [`NOTICE`](NOTICE) and credit
  [Akasha OS](https://github.com/azerothl/akasha-os).
- **Commercial license** ([`LICENSE-COMMERCIAL.md`](LICENSE-COMMERCIAL.md)) —
  proprietary forks / closed SaaS. Attribution + royalty. Contact:
  loic.peaudecerf@proton.me.

The **Akasha OS** trademark is reserved.

## Status

Preview cohort (PC) is in progress. Full gate tables:
**[docs/STATUS.md](docs/STATUS.md)**.
