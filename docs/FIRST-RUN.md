# First run — Agent OS Preview

**Language:** English | [Français](fr/FIRST-RUN.md)

**This is not a bootable OS.** Preview runs on Windows or Linux x64 with an
**NVIDIA** GPU.

## Before you launch

1. Recent NVIDIA driver (`nvidia-smi -L` OK).
2. ~4 GB free (binaries + GGUF models downloaded on first run).
3. Install via `install.ps1` / `install.sh`, or run `bin/aos-session`.

## What the first launch does

1. Checks NVIDIA and disk space.
2. Downloads Qwen2.5 models (3B + 0.5B) into `share/models/` if missing
   (network required **once**).
3. Starts services and opens the egui UI.
4. Shows the **tutorial** (onboarding): language, trust, tab tour.

## What you can do

| Tab | Usage |
|-----|--------|
| Chat / Sessions | Parallel persisted conversations |
| Memory | Long-term facts (remember / recall) |
| Notes | Human notes + via agent |
| Agents | Tasks with skills / tools |
| Network (sidebar checkbox) | Opt-in for web search / downloads |
| Feedback | GitHub issue on azerothl/akasha-os |
| Scenarios | Cohort protocol (see TESTER.md) |

In-app network is **off** by default. Software updates use GitHub Releases
(UI banner) without wiping `var/`.

## Quick troubleshooting

- No GPU → install the NVIDIA driver (no CPU mode in 0.1).
- Missing model → let first run download, or copy GGUFs into `share/models/`.
- Daemon logs → `var/run/*.stderr.log` (Troubleshooting button in the UI).
