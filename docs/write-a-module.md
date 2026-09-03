# First module (no cargo)

**Language:** English | [Français](fr/write-a-module.md)

> Date: 03/09/2026 · Preview **0.17.0**

A module is dual-surface: tools for agents, a closed widget tree for the
human. Preview can scaffold, package, and install a **script** module
without cargo. Longer than a [skill](write-a-skill.md); still not a kernel
PR ([ADR 0006](../adr/0006-license-split.md)).

Site chapter:
[azerothl.github.io/akasha-os/docs/module.html](https://azerothl.github.io/akasha-os/docs/module.html).

## What this is not

Not `notes` / `tasks` / `canvas` / `ext-rt` (bundled). Not HTML/JS or a
webview (`declarative_ui` only). Not a public marketplace — local signed
catalogue plus cap review.

## Path A — ask the agent

1. Preview installed ([INSTALL.md](INSTALL.md);
   [15-minute path](TESTER.md#short-path-15-minutes)).
2. Name: 2–32 chars, `[a-z][a-z0-9-]*`, not a bundled name
   (`cohortmod`, `pingmod`).
3. Scenarios → **Launch agent: create module cohortmod**, or Chat
   “create a module pingmod” / « crée un module pingmod ».
4. **Cap review**: read the list (often
   `fs.write:/documents/<name>/**` and `module.install`). Deny = fail-closed;
   timeout = deny; Accept to install.
5. Sidebar **Modules → &lt;name&gt;** (not Notes/Tasks). Heading, form or
   button, table. Submit once.

## Where files go

Under the Preview home `var/modules/`:

- source: `src/<name>/` (`handlers.yaml` for kind `script`)
- package: `packages/<name>.aospkg`

Windows: `%LOCALAPPDATA%\AgentOS-Preview`. Linux / macOS:
`~/.local/share/agentos-preview`.

## Path B — Settings

Settings → Modules: `module.scaffold` (kind `script`) → `module.package` →
`module.install`. Same cap review. Uninstall: Settings → Installed modules
(not the bundled four).

## Rust compile (not first)

`module.scaffold` kind `rust` + `module.compile`: critical cap, static
refuse of `unsafe` / ambient fs/net/process, `CARGO_NET_OFFLINE=true`.
SDK: [`modules/sdk`](../modules/sdk) (Apache-2.0). Skip until a script
module has run.

## Share

Paste or attach `var/modules/packages/<name>.aospkg` in
[Show and tell](https://github.com/azerothl/akasha-os/discussions) — not a
CLA. PRs under `community/modules/` only, never `crates/`. Install still
runs cap review. See [community.md](community.md).
