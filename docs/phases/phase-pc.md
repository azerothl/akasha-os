# Phase PC — Preview cohort

**Language:** English | [Français](../fr/phases/phase-pc.md)

## Goal

Ship **Agent OS Preview** to a cohort: install on Windows / Linux x64
(NVIDIA recommended; CPU path OK) or macOS Apple Silicon, test from egui,
send local feedback. **Not** seL4 / bare metal.

## Deliverables

| # | Deliverable | Status |
|---|-------------|--------|
| PC.1 | `aos-session` | done |
| PC.2 | Win/Linux/macOS packaging | GitHub Actions CI + model download |
| PC.3 | egui cohort UI | done (+ tutorial) |
| PC.4 | `feedback.submit` | local + GitHub issue |
| PC.5 | INSTALL + TESTER + FIRST-RUN | done |
| PC.10 | Non-destructive Release updates | done |
| PC.6 | Parallel persisted chat sessions | done |
| PC.7 | Session + user memory + UI panel | done |
| PC.8 | `web.search` + opt-in network | done |
| PC.9 | Binary FS + `net.fetch` + `files.generate` | done |
| PC.11 | Agent transparency panel (`agent.trace`, sources, steer) | done |
| PC.12 | Settings tab + `var/run/preferences.json` | done |
| PC.13 | Multi-engine `web.search` + `web.browse` | done |
| PC.14 | Memory-first bootstrap + Qwen `<think>` strip | done |

Catalogue: [`docs/FEATURES.md`](../FEATURES.md).

## Gates PC.6–PC.9

| Gate | Criterion |
|------|-----------|
| PC.6 | 3 concurrent sessions; history survives restart |
| PC.7 | `mem.context` injected into infer; remember / recall UI |
| PC.8 | search OK when online; refused under `offline_strict` |
| PC.9 | image download + PDF generated under `/downloads` |

See scenarios in [`docs/TESTER.md`](../TESTER.md).

## Cohort gate

3 Windows + 1 Linux + 1 macOS Apple Silicon complete the
[15-minute path](../TESTER.md#short-path-15-minutes) without a toolchain;
≥1 feedback each. Long TESTER protocol remains the team checklist
(PC.6–PC.9, PC.11–PC.13).

```powershell
.\packaging\build-preview.ps1
```

## Next

- Wave 2 leftovers: zip export, cancel infer/fetch, clipboard
- Pilot cohort + feedback triage (gate still open)
- PV / bare metal remain parallel (ADR 0001)
