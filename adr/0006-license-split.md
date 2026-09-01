# ADR 0006: Community license split (kernel vs extensions)

**Language:** English | [Français](../docs/fr/adr/0006-license-split.md)

> Date: 01/09/2026 · Status: **accepted**

## Context

Akasha OS is dual-licensed: **AGPL-3.0-only** (`LICENSE`) and a **commercial**
offer (`LICENSE-COMMERCIAL.md`). [CONTRIBUTING.md](../CONTRIBUTING.md) required
every pull request into this repository to (1) place the work under AGPL-3.0-only
and (2) grant Loïc Peaudecerf an irrevocable license to redistribute it under
the commercial terms.

That inbound grant is necessary so the **host OS** (daemons, caps, UI, packaging)
can stay dual-licensed. It is toxic for an extension ecosystem: an author of a
Markdown skill or a dual-surface `.aospkg` will not grant a commercial SaaS
right in order to share a morning-brief recipe.

Horizon 0 already treats a GitHub Discussion as **not** a CLA event. That is
not enough. Authors need a path to land code in git without the commercial
grant, while the kernel dual-license stays intact.

Guest modules talk to the host only through `host_call` (see `modules/sdk`).
They are separate works that consume a cap-gated ABI. They must not become
AGPL-derived merely by running on Preview.

This ADR does **not** create a public marketplace (still out of Preview; E10
is a signed catalogue later). It does not relicense the host.

## Options

### A — Status quo (every PR is AGPL + commercial grant)

Keeps dual-licensing trivial. Kills third-party skills and modules. Discussions
remain the only safe share path.

### B — Second repository only (kernel here, all extensions in another repo)

Clean legal boundary. Premature at current scale (one maintainer, Preview
cohort still open). A catalogue repo remains allowed later; it must not be
the only way to share a skill this month.

### C — Path-based split in this repository (chosen)

Kernel/host paths stay AGPL + inbound commercial grant. Guest SDK and
community extensions use permissive licenses **without** that grant.
A later catalogue repository, if any, follows the same extension rules.

### D — Relicense the whole OS MIT/Apache

Would grow a kernel contributor pool and destroy the commercial dual-license
of the host. Rejected.

## Decision

**Option C.** Treat the host as the copyleft OS, and guest extensions as
userspace that authors license themselves.

| Artefact | License | Inbound commercial grant (CLA) |
|----------|---------|--------------------------------|
| Host OS: `crates/`, `packaging/`, `website/`, host UI, daemons, `vm/`, `demo/` | AGPL-3.0-only + commercial offer | **Yes** — every PR |
| Docs that describe the host (`docs/`, `adr/`, root README / NOTICE / license files) | Same as host (repo default) | **Yes** |
| Guest SDK: `modules/sdk` | **Apache-2.0** | **No** |
| First-party guest modules: `modules/notes`, `modules/tasks`, `modules/canvas`, `modules/ext-rt` (and their `.aospkg`) | **Apache-2.0** | **No** (after relicense) |
| Shipped first-party skills: `skills/`, `share/skills/` | **MIT** | **No** (after relicense) |
| Community extensions: `community/` (skills, `.aospkg`, examples) | Author’s OSI license, **default MIT** | **No** |
| GitHub Discussion / in-app Feedback | n/a | **No** |
| Future signed catalogue repo (E10) | Index Apache-2.0; each package keeps its license | **No** |

The copyright holder (Loïc Peaudecerf) **authorizes** relicensing the guest SDK,
first-party guest modules, and shipped skills as in the table. SPDX identifiers
and `Cargo.toml` `license` fields for those trees are Apache-2.0 / MIT as of
the follow-up that landed `LICENSE-APACHE`, `LICENSE-MIT`, and `community/`.

### Why Apache-2.0 for the SDK (and first-party modules)

The SDK is the ABI authors compile against. Apache-2.0 gives an explicit
patent grant and does not copyleft a third-party module. First-party modules
are the templates people will copy; leaving them AGPL would trap “my first
module” under the kernel CLA.

Shipped skills are Markdown recipes. MIT is the lowest-friction default for
text authors will fork.

### Why the host stays AGPL + CLA

The commercial license is an **alternative** to AGPL for proprietary forks and
closed SaaS of the OS. Inbound CLA on host paths is the only way third-party
kernel patches remain redistributable on that path. This ADR does not weaken
that.

### Guest vs host (not a derived work by mere execution)

A module that only uses the published `host_call` ABI, declared caps, and
closed `declarative_ui` widgets is **not** an AGPL derivative of the host
solely because it is installed on Preview. Combining a module *into* the
host binary or vendoring `crates/` is a different fact pattern and stays
AGPL.

## Consequences

### Policy (effective when CONTRIBUTING cites this ADR)

- A PR that only touches Apache/MIT trees does **not** grant the commercial
  license. The contributor certifies they have the right to submit and places
  the work under the license of that tree (Apache-2.0 or MIT / SPDX in the
  files).
- A PR that touches AGPL trees is still AGPL + commercial grant, as today.
- A mixed PR is rejected or split: AGPL files in one PR, permissive files in
  another.
- A Show and tell Discussion remains the zero-CLA share path even after
  `community/` exists.
- The **Akasha OS** trademark stays reserved. A community module or fork must
  not take that product name (`LICENSE-COMMERCIAL.md` §3).

### Implementation follow-up

1. ~~Add `LICENSE-APACHE` / `LICENSE-MIT`; Apache-2.0 on guest `Cargo.toml`; SPDX on those crates.~~
2. ~~Mark shipped `skills/*/SKILL.md` as MIT.~~
3. ~~Create `community/` with a README (default MIT, not the host CLA).~~
4. ~~Rewrite [CONTRIBUTING.md](../CONTRIBUTING.md) (and `docs/fr/CONTRIBUTING.md`).~~
5. Keep Preview zip contents honest: include `LICENSE-APACHE` and `LICENSE-MIT`
   next to `LICENSE`; bundled notes/tasks are Apache; host binaries remain
   AGPL. `NOTICE` describes both layers.
6. A network signed catalogue (E10) may come later; it must not require the
   commercial grant. Cap review on install is unchanged.

### Out of scope

- Public module store, payments, or ClawHub clone
- Relicensing `crates/` or dropping the commercial offer
- Treating MCP servers the user added as Akasha OS works
- seL4 / bare-metal licensing (same host dual-license when that code lives
  under `crates/` or `vm/` as part of the OS)

## Notes

`docs/technical-specs.md` listed a placeholder `adr/0006-wasm-modules.md`.
That file was never written. **0006 is this license split.** A future WASM
ABI ADR, if needed, takes the next free number.
