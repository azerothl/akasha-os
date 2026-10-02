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

## P21 — Preview 0.19.0 (server daemon + Dev-assistant P0) — done

Track A (`aos-serverd` **P21.0–P21.6**) and Track B (#247 **DA.1–DA.4**)
landed on `main` (#406 / #407 / #409 / #410). **P21.7** is this release
docs / VERSION cut. Do **not** invent a P6 number (PC still open). No new
E* number for these host tracks. #247 P1 starts with **DA.5** (read-only git); `git.commit` / `process.run` / IDE stay **0.20+**.

| # | Item | Status |
|---|------|--------|
| P21 / #403 | **`aos-serverd`** — ADR 0012 + lifecycle + headless serve + watchdogs + local control + **agent intake** (P21.0–P21.4) | **done** (#406) |
| P21 / #403 | **P21.5** — opt-in OS services (systemd user / launchd / Windows logon task) + ship `aos-serverd` in Preview zips | **done** (#409) |
| P21 / #403 | **P21.6** — opt-in supervise `aos-mcpd` / `aos-bridged` ; session attach UI only | **done** (#410) |
| P21 / #247 | **Dev-assistant P0** — bind + search + apply_patch/undo + [module contract](dev-assistant-p0.md) + community reference module | **done** (#407) |
| P21 / #403 | **P21.7** — VERSION 0.19.0, FEATURES/STATUS/website/TESTER headless, packaging honesty | **done** |

## P22 — Preview 0.20 (Dev-assistant P1 start) — in progress

| # | Item | Status |
|---|------|--------|
| P22 / #247 | **DA.5** — read-only `git.status` / `git.diff` on bound workspaces (`fs.read` gated) | **done** (this PR) |
| P22 / #247 | `git.commit`, cap-gated `process.run`, Problems list, LSP/DAP, DeclUI IDE | backlog |
