# Write a skill in ten minutes

**Language:** English | [Français](fr/write-a-skill.md)

> Date: 01/09/2026 · Preview **0.15.1**

A skill is a Markdown recipe. You can copy one into Preview without cargo,
without a kernel pull request, and without granting the commercial license
([ADR 0006](../adr/0006-license-split.md)).

Site chapter (same procedure):
[azerothl.github.io/akasha-os/docs/skill.html](https://azerothl.github.io/akasha-os/docs/skill.html).

## What this is not

Not a WASM module, not a crate, not the five skills in the zip
(`share/skills/`). Yours go under `var/skills/` so an update overlay does
not wipe them.

## Steps

1. Preview already installed ([INSTALL.md](INSTALL.md); cohort
   [15-minute path](TESTER.md#short-path-15-minutes)).
2. Folder name = skill id: lowercase, digits, hyphens; 2–33 characters;
   starts with a letter (`morning-brief`, `ping`).
3. File: Preview home `var/skills/<name>/SKILL.md` — not `share/skills/`.
4. Restart Preview. Start an agent with that skill, or ask in a way that
   matches `when_to_use`.

| OS | Destination |
|----|----------------|
| Windows | `%LOCALAPPDATA%\AgentOS-Preview\var\skills\<name>\SKILL.md` |
| Linux / macOS | `~/.local/share/agentos-preview/var/skills/<name>/SKILL.md` |

Preview loads **`SKILL.md` only**. A `SKILL.fr.md` is for humans.

## Path A — copy the example

[`community/skills/morning-brief/SKILL.md`](../community/skills/morning-brief/SKILL.md)
→ `var/skills/morning-brief/SKILL.md`. Ask “Give me a morning brief”.
Local only: memory, open tasks, notes. No network.

## Path B — write ping

Save as `var/skills/ping/SKILL.md`. Agent body is English, like shipped
skills. Keep `license: MIT`. Ask “ping”.

```markdown
---
name: ping
description: Reply with pong and stop
license: MIT
when_to_use: User says ping or asks if a custom skill is loaded.
tools:
  - goal.complete
---
# Ping

Reply with the single word pong. Then call goal.complete.
Do not search the web. Do not create notes or tasks.
```

## Share

[Show and tell](https://github.com/azerothl/akasha-os/discussions) with the
file pasted in — not a CLA. A PR belongs under `community/` only, never
`crates/`. See [community.md](community.md) and
[CONTRIBUTING.md](../CONTRIBUTING.md). Next:
[first module](write-a-module.md) (still no cargo).
