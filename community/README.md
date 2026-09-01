# Community extensions

**Language:** English | [Français](#francais)

This tree is **not** the Akasha OS host. It is for skills, `.aospkg` modules,
and examples that people share without granting the commercial license.

Policy: [ADR 0006](../adr/0006-license-split.md).

| Path | License | Commercial CLA |
|------|---------|----------------|
| This directory (`community/`) | **MIT** by default ([`LICENSE-MIT`](../LICENSE-MIT)); an author may pick another OSI license in the files | **No** |
| Guest SDK / first-party WASM (`modules/`) | Apache-2.0 ([`LICENSE-APACHE`](../LICENSE-APACHE)) | **No** |
| Host OS (`crates/`, UI, packaging, website) | AGPL-3.0-only + commercial offer | **Yes** |

A GitHub Discussion (Show and tell) is always a valid way to share a skill
or module without a PR.

Do not name a fork or package **Akasha OS**. The trademark stays reserved.

Worked example: [`skills/morning-brief/`](skills/morning-brief/) — copy
`SKILL.md` into Preview `var/skills/morning-brief/` (not shipped in the zip).

## How to add a skill

1. Copy [`skills/morning-brief/SKILL.md`](skills/morning-brief/SKILL.md) or a
   shipped recipe such as [`skills/planner/SKILL.md`](../skills/planner/SKILL.md).
2. Put yours under `community/skills/<name>/SKILL.md` with `license: MIT` (or
   your OSI license) in the YAML frontmatter.
3. Open a PR that **only** touches `community/` (no `crates/`).

## How to add a module

Scaffold in Preview (`module.scaffold` / package), keep the `.aospkg` here
under `community/modules/`, and list required caps in the manifest. Install
in Preview still runs cap review. Guest code compiles against
[`modules/sdk`](../modules/sdk) (Apache-2.0).

---

## Français

Cet arbre **n’est pas** l’hôte Akasha OS. Il sert aux skills, modules
`.aospkg` et exemples partagés **sans** octroi de licence commerciale.

Politique : [ADR 0006](../adr/0006-license-split.md). Défaut **MIT**
([`LICENSE-MIT`](../LICENSE-MIT)). Exemple :
[`skills/morning-brief/`](skills/morning-brief/) — copier `SKILL.md` vers
`var/skills/morning-brief/`. Une Discussion Show and tell reste valable sans
PR. Une PR ici ne doit pas toucher `crates/`. La marque Akasha OS reste
réservée.
