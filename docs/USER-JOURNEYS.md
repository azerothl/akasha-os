# User journeys — Preview acceptance suite

End-user journeys against a **running Preview** with **real media engines**
(not stubs). Complements the cohort [TESTER.md](TESTER.md) short path: these
checks exercise session context, image/video completion, and result access via
the bus, then optionally refresh marketing screenshots under `website/media/`.

## Prerequisites

- Windows Preview install (or a checkout with the same layout), typically
  `%LOCALAPPDATA%\AgentOS-Preview`
- Real engines: `bin/sd.exe` (+ CUDA DLLs as shipped)
- Weights: at least `share/models/v1-5-pruned-emaonly.safetensors`
  (`local:sd-v1-5`)
- For video (J3): `local:ltx2.3-dev` weights + sidecars (or pass `-SkipVideo`)
- Rust toolchain only if you need to rebuild `preview_probe` / `aos-ui-egui`

## Run

From the repo root (Preview must expose a healthy bus with `platformd` +
`modeld` — launch the installed Preview once if needed):

```powershell
# Prefer: start Akasha OS Preview normally, then:
.\demo\run-user-journeys.ps1 -NoStart -Screenshots

# Full suite including short video + restart
.\demo\run-user-journeys.ps1 -Screenshots

# Faster / no LTX / no restart
.\demo\run-user-journeys.ps1 -NoStart -SkipVideo -Screenshots -NoRestart
```

If `aos-session` shows a startup failure dialog, repair corrupted daemons under
`%LOCALAPPDATA%\AgentOS-Preview\bin` (e.g. recopy `aos-auditd.exe`) and ensure
`aos-modeld.exe etc/modeld.yaml` is running before `-NoStart`.

Environment:

| Variable | Meaning |
|----------|---------|
| `AOS_HOME` | Preview home (defaults to AgentOS-Preview when SD weights are present) |
| `AOS_PROBE_BUS` | Bus (`127.0.0.1:24701`) |
| `AOS_SESSION_EXE` | Override `aos-session.exe` |

## Journeys

| ID | What it proves |
|----|----------------|
| **J1** | Create 3 sessions, append distinct context, switch B→A: A still has its marker |
| **J2** | `media.image.generate` with `local:sd-v1-5` → real PNG (`engine≠stub`) under `/downloads` |
| **J3** | Short `vid_gen` (256², 9 frames) → WebM EBML signature |
| **J4** | Result listed via `fs.list` + recorded/listed in Create history / document state |
| **J5** | Stop/restart Preview → sessions + marker still on disk |

Fixtures: [`demo/user-journeys/`](../demo/user-journeys/). Orchestrator:
[`demo/run-user-journeys.ps1`](../demo/run-user-journeys.ps1).

**Duration:** J2 is usually under a few minutes on NVIDIA; J3 dominates (minutes
to tens of minutes depending on GPU). Prefer `-SkipVideo` for a quick smoke.

## Screenshots (`-Screenshots`)

Builds `aos-ui-egui` with `AOS_UI_SCREENSHOT_FOCUS=marketing`, captures:

- `m1-rail.png` → `website/media/rail.png`
- `m2-chat.png` → `website/media/chat.png`
- `m3-create.png` → `website/media/create.png`

Optional: `AOS_UI_SCREENSHOT_RESULT=/downloads/images/…` to show the J2 PNG in
the Create preview pane.

## Reports

- `var/recette/user-journeys-<date>.json`
- `docs/recette-user-journeys-<date>.md`

Success line: `AOS_USER_JOURNEYS_PASS`.

## Manual UI checklist (bus cannot click egui)

After a green run, spot-check once:

1. Chat sidebar: switch sessions A → B → A; histories match.
2. Create: result visible in Preview + History row.
3. Chat `/image …` → attachment + **Open in studio**.
4. Files tab: `/downloads` shows the generated file.

## Related

- Previous IPC recette: [recette-preview-2026-09-09.md](recette-preview-2026-09-09.md)
- Create contract: [create-contract.md](create-contract.md)
- Cohort protocol: [TESTER.md](TESTER.md)
