# Contributing to Akasha OS

**Language:** English | [Français](docs/fr/CONTRIBUTING.md)

Thank you. You do not need `cargo` or a clone to help.

The Preview cohort is open. Gate: **3 Windows + 1 Linux + 1 macOS Apple
Silicon** testers complete the [15-minute path](docs/TESTER.md#short-path-15-minutes)
without a Rust toolchain, each with a usable `var/feedback/` report.

## Preview cohort (preferred)

1. Install from [GitHub Releases](https://github.com/azerothl/akasha-os/releases)
   — see [docs/INSTALL.md](docs/INSTALL.md). macOS builds are unsigned; run
   `install.sh` and expect Gatekeeper. Intel Mac is not supported.
2. Follow the [short path](docs/TESTER.md#short-path-15-minutes).
3. Send feedback from the UI (Feedback tab). Keep **Create a GitHub issue**
   checked, except for security.
4. Optionally check in on
   [Discussions](https://github.com/azerothl/akasha-os/discussions).

The long protocol in [docs/TESTER.md](docs/TESTER.md) is the team checklist
for PC.6–PC.9 and PC.11–PC.13. It is not required of every tester.

Public hangout: GitHub Discussions. There is no Discord (or other messaging
channel) for this project yet. Community page:
[docs/community.md](docs/community.md).

## Ways to help without a kernel PR

| Path | Where | License today |
|------|-------|----------------|
| Tester report | In-app Feedback → GitHub issue | n/a |
| Question / cohort check-in | [Discussions](https://github.com/azerothl/akasha-os/discussions) | n/a |
| Skill or module idea | Discussions (Show and tell) | Sharing in a Discussion does **not** grant the commercial license |
| Docs typo / translation | Pull request | Contribution license below |
| ADR proposal | Pull request under `adr/` | Contribution license below |
| Kernel / crate change | Pull request | Contribution license below |

Until a later license split, **any file you PR into this repository** is
covered by the contribution license below. Prefer a Discussion if you want
to share a skill or module without that grant.

## How to propose a skill

Skills are Markdown under `share/skills/` / `var/skills/` (see a shipped
example such as [`skills/planner/SKILL.md`](skills/planner/SKILL.md)). Open a
Show and tell Discussion with the `SKILL.md` body. Do not open a PR unless
you accept the contribution license.

## How to scaffold a module

In Preview: Settings → Modules, or ask an agent to `module.scaffold` (TESTER
steps 14–15). Share the `.aospkg` in a Discussion. Install still runs cap
review. Guest SDK: [`modules/sdk`](modules/sdk).

## How to propose an ADR

Copy the style of [`adr/0001-microkernel.md`](adr/0001-microkernel.md). Open a
PR titled `adr: …`. The contribution license applies.

## Labels we use

`bug`, `cohort`, `area:modules`, `area:skills`, `good first issue`. Issue
forms set some of these; Discussions stay unlabelled unless a maintainer
adds one.

## Code of conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Security

See [SECURITY.md](SECURITY.md). Do not file a public issue.

## Contribution license

The project is **dual-licensed** (AGPL-3.0-only + commercial). For both
paths to remain viable, every contribution **merged into this repository**
must be redistributable under both regimes.

By opening a pull request or pushing code to this repository, you certify
that:

1. you own the rights to your contribution, or you have permission to submit
   it;
2. you place it under **GNU AGPL-3.0-only** (`LICENSE`);
3. you grant the licensor (Loïc Peaudecerf) an irrevocable, worldwide,
   royalty-free license to redistribute your contribution also under the
   **commercial license** described in `LICENSE-COMMERCIAL.md`.

If you cannot grant point 3, do not submit the contribution here; discuss a
written exception first. A Discussion post is not a contribution under this
section.

## Trademark

Do not use “Akasha OS” as the name of a fork. Keep `NOTICE`
and a link to <https://github.com/azerothl/akasha-os>.
