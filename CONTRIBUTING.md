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

License policy: [ADR 0006](adr/0006-license-split.md) (kernel vs extensions).

| Path | Where | License / CLA |
|------|-------|----------------|
| Tester report | In-app Feedback → GitHub issue | n/a |
| Question / cohort check-in | [Discussions](https://github.com/azerothl/akasha-os/discussions) | n/a |
| Skill or module idea | Discussions (Show and tell) or [`community/`](community/README.md) PR | **No** commercial grant. Discussion is always safe. `community/` default MIT |
| Guest SDK / first-party WASM module | PR under `modules/` | Apache-2.0 ([`LICENSE-APACHE`](LICENSE-APACHE)); **no** commercial grant |
| Docs typo / translation | Pull request under `docs/` | Host CLA (AGPL + commercial grant) |
| ADR proposal | Pull request under `adr/` | Host CLA |
| Kernel / crate / host UI | Pull request under `crates/`, `packaging/`, `website/` | Host CLA |

Do **not** mix AGPL host files and permissive extension files in one PR.

## How to propose a skill

Skills are Markdown under `share/skills/` / `var/skills/` (see a shipped
example such as [`skills/planner/SKILL.md`](skills/planner/SKILL.md)). Open a
Show and tell Discussion with the `SKILL.md` body, or a PR under
[`community/`](community/README.md) (MIT default, no commercial grant). A PR
that changes shipped `skills/` in the Preview zip is MIT — still no
commercial grant.

## How to scaffold a module

In Preview: Settings → Modules, or ask an agent to `module.scaffold` (TESTER
steps 14–15). Share the `.aospkg` in a Discussion (no CLA) or under
[`community/`](community/README.md). Install still runs cap review. Guest SDK:
[`modules/sdk`](modules/sdk) (Apache-2.0).

## How to propose an ADR

Copy the style of [`adr/0001-microkernel.md`](adr/0001-microkernel.md). Open a
PR titled `adr: …`. ADRs live with the host docs: the contribution license
for AGPL paths applies.

## Labels we use

`bug`, `cohort`, `area:modules`, `area:skills`, `good first issue`. Issue
forms set some of these; Discussions stay unlabelled unless a maintainer
adds one.

## Code of conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Security

See [SECURITY.md](SECURITY.md). Do not file a public issue.

## Contribution license

The **host OS** is dual-licensed (AGPL-3.0-only + commercial). Guest
extensions are not. See [ADR 0006](adr/0006-license-split.md).

By opening a pull request that touches **host** paths (`crates/`,
`packaging/`, `website/`, `docs/`, `adr/`, `vm/`, `demo/`, and other AGPL
trees), you certify that:

1. you own the rights to your contribution, or you have permission to submit
   it;
2. you place it under **GNU AGPL-3.0-only** (`LICENSE`);
3. you grant the licensor (Loïc Peaudecerf) an irrevocable, worldwide,
   royalty-free license to redistribute your contribution also under the
   **commercial license** described in `LICENSE-COMMERCIAL.md`.

If you cannot grant point 3, do not submit a host-path PR; discuss a written
exception first.

By opening a pull request that only touches **extension** paths
(`modules/sdk`, first-party guest modules, `skills/`, `community/`), you
certify that you have the right to submit and that you place the work under
the license of that tree (Apache-2.0 or MIT, as in ADR 0006). That PR does
**not** grant the commercial license.

A Discussion post is never a contribution under this section.

## Trademark

Do not use “Akasha OS” as the name of a fork. Keep `NOTICE`
and a link to <https://github.com/azerothl/akasha-os>.
