# morning-brief

**Language:** English | [Français](#francais)

Example **community** skill (MIT). It is **not** in the Preview zip. Copy it
into `var/skills/` yourself. Policy: [ADR 0006](../../../adr/0006-license-split.md).

The agent recipe is [`SKILL.md`](SKILL.md) (English). Human translation:
[`SKILL.fr.md`](SKILL.fr.md). Preview loads only `SKILL.md`.

## Install

Need `SKILL.md` in a folder named `morning-brief` under the Preview **home**
`var/skills/` (not `share/skills/` — that is the shipped set).

| OS | Destination |
|----|----------------|
| Windows | `%LOCALAPPDATA%\AgentOS-Preview\var\skills\morning-brief\SKILL.md` |
| Linux / macOS | `~/.local/share/agentos-preview/var/skills/morning-brief/SKILL.md` |

From a git clone (with `AOS_HOME` pointing at the repo):

```
community/skills/morning-brief/SKILL.md  →  var/skills/morning-brief/SKILL.md
```

Restart Preview, or run `skill.list`. Start an agent with skill
`morning-brief`, or ask “Give me a morning brief”. Ten-minute procedure:
[write a skill](https://azerothl.github.io/akasha-os/docs/skill.html)
([`docs/write-a-skill.md`](../../../docs/write-a-skill.md)).

Do **not** PR this into `crates/` or `skills/` (the shipped zip). A change
to the example stays under `community/`.

---

## Français

Skill **communautaire** d’exemple (MIT). **Pas** dans le zip Preview. Copier
`SKILL.md` vers `var/skills/morning-brief/` (chemins ci-dessus). La recette
agent est en anglais ; [`SKILL.fr.md`](SKILL.fr.md) est pour les humains.
