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

Worked example: [`skills/morning-brief/`](skills/morning-brief/) — listed in
the signed index [`catalogue.yaml`](catalogue.yaml) (Apache-2.0 index; this
skill stays MIT). Preview can install it from **Settings → Local module
catalogue** after you enable the opt-in community source (cap review
unchanged). Manual copy into `var/skills/morning-brief/` still works.
Ten-minute procedure (site):
[write a skill](https://azerothl.github.io/akasha-os/docs/skill.html). Repo
copy: [`docs/write-a-skill.md`](../docs/write-a-skill.md). Policy:
[ADR 0007](../adr/0007-signed-git-catalogue.md).

## How to add a skill

1. Copy [`skills/morning-brief/SKILL.md`](skills/morning-brief/SKILL.md) or a
   shipped recipe such as [`skills/planner/SKILL.md`](../skills/planner/SKILL.md).
2. Put yours under `community/skills/<name>/SKILL.md` with `license: MIT` (or
   your OSI license) in the YAML frontmatter.
3. Add an entry to [`catalogue.yaml`](catalogue.yaml): `kind: skill`, path,
   `sha256:` of `SKILL.md`, `license`, `attested_caps` (usually empty).
   Resign: `UPDATE_COMMUNITY_CATALOGUE=1 cargo test -p aos-platform --lib community_catalogue_signature_matches`.
4. Open a PR that **only** touches `community/` (no `crates/`).

Preview testers: Settings → Local module catalogue → **Enable community
catalogue** → Fetch community index → Install. Tampered index = refuse.

## How to add a module

Procedure (site):
[first module](https://azerothl.github.io/akasha-os/docs/module.html). Repo:
[`docs/write-a-module.md`](../docs/write-a-module.md). Scaffold in Preview
(`module.scaffold` kind `script` / package / install), keep the `.aospkg`
under `community/modules/`, list required caps in the manifest **and**
`attested_caps` in [`catalogue.yaml`](catalogue.yaml), then resign the
index (same command as for a skill). Install in Preview still runs cap
review. Guest code compiles against [`modules/sdk`](../modules/sdk)
(Apache-2.0).

---

## Français

Cet arbre **n’est pas** l’hôte Akasha OS. Il sert aux skills, modules
`.aospkg` et exemples partagés **sans** octroi de licence commerciale.

Politique : [ADR 0006](../adr/0006-license-split.md). Index signé
Apache-2.0 : [`catalogue.yaml`](catalogue.yaml) ([ADR 0007](../adr/0007-signed-git-catalogue.md)).
Défaut **MIT** ([`LICENSE-MIT`](../LICENSE-MIT)). Exemple :
[`skills/morning-brief/`](skills/morning-brief/) — activer le catalogue
communautaire dans Settings, ou copier `SKILL.md` vers
`var/skills/morning-brief/`. Guide module :
[premier module](https://azerothl.github.io/akasha-os/docs/module.html?lang=fr).
Une Discussion Show and tell reste valable sans PR. Une PR ici ne doit pas
toucher `crates/`. La marque Akasha OS reste réservée.
