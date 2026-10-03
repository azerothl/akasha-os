# Project status

**Language:** English | [Français](fr/STATUS.md)

Summary of delivered phases. Detail: [development-plan.md](development-plan.md),
[phases/](phases/). Shipped Preview surface: [FEATURES.md](FEATURES.md).

**Headline:** P0 ✅ / P1 ✅ / P2 ✅ / P3 ✅ / P4 ✅ / PV.1–PV.3 ✅ / P5.1 ✅ / PC 🚧

**Preview:** 0.19.0 — **`aos-serverd`** headless lifecycle + agent intake
([#403](https://github.com/azerothl/akasha-os/issues/403)), opt-in OS
services and mcpd/bridged supervise, session attach UI-only; Dev-assistant
**P0** `workspace.bind` / search / apply_patch ([#247](https://github.com/azerothl/akasha-os/issues/247)).
Builds on 0.18.0 (Memory V2, E22/E23, harness + `aos-mcpd`, Illustration
Studio 0.7.23). Not a bootable OS. Cohort gate still open
(**3 Windows + 1 Linux + 1 macOS Apple Silicon**, 15-minute path).
**Next:** PC cohort close; #247 **DA.5** read-only `git.status`/`git.diff` landed
(start of 0.20 P1); remaining P1–P2 (`git.commit`, `process.run`, LSP/DAP,
DeclUI IDE) still **0.20+**; Horizon C / PV.4+ when scheduled; E9 hard-green
after a documented 2-GPU run.

## P22 — Preview 0.20 (Dev-assistant P1 start) — in progress

| # | Item | Status |
|---|------|--------|
| P22 / #247 | **DA.5** — read-only `git.status` / `git.diff` on bound workspaces (`fs.read` gated) | **done** |
| P22 / #432 | Mix Path A host + song-maker module stub | **done** (this PR) |
| P22 / #433 | Host ASR → untrusted gate text (no bundled STT) | **done** (this PR) |
| P22 / #247 | `git.commit`, cap-gated `process.run`, Problems list, LSP/DAP, DeclUI IDE | backlog |

## Dev commands

```powershell
cargo test --workspace
```
