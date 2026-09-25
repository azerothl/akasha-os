# ADR 0008: Shared skill frontmatter profile

**Language:** English | [Français](../docs/fr/adr/0008-shared-skill-frontmatter-profile.md)

> Date: 25/09/2026 · Status: **accepted**

## Context

- Alignment between Akasha ([azerothl/Akasha](https://github.com/azerothl/Akasha) personal assistant) and akasha-os Preview skills.
- Recipe *content* may converge; loaders, tool ids, and WASM ABIs stay separate.
- No shared loader. Dual-publish of packages is on hold.
- Frozen companion doc will live in Akasha_skills (akasha-dev); leave a Related placeholder linking `azerothl/Akasha_skills` when available.

## Decision

Frontmatter contract only (one `SKILL.md` YAML):

- **Single file:** portable common fields + product-specific keys in the same frontmatter; each loader reads what it knows and **ignores unknown keys**.

**Common (portable):**

- `name` (required) — kebab-id AND folder name `var/skills/<name>/`; Preview regex `[a-z][a-z0-9-]{1,32}`
- `description` (required)
- `license` (recommended; MIT for community skills per [ADR 0006](0006-license-split.md))
- `when_to_use` (optional; fall back to `description`)
- `runtime` (optional catalogue filter) — **YAML list only**: `[akasha]` | `[akasha-os]` | `[akasha, akasha-os]`. Never the token `both`. Absent = not filtered / author unspecified.

**Product keys in the same file:**

- **aos / Preview:** `tools`, `required_caps` (keep `required_caps` separate from `tools`; catalogue install still fail-closed cap review when non-empty — see `crates/aos-platform/src/skill.rs`)
- **Akasha:** `compatibility`, `metadata`, and other Akasha-only keys

### Out of scope / non-goals

- No monorepo merge
- No shared crates / glue
- No WASM ABI fusion
- No dual-publish pipeline for now
- Not changing DeclUI / `.aospkg` module contracts

## Consequences

- Authors can write one `SKILL.md` that both ecosystems can store; each runtime interprets product keys independently.
- Preview loader may ignore `runtime` / `license` until catalogue filtering needs them (unknown keys already ignored).
- Follow-up (optional, not this PR): parse full `SKILL.md` frontmatter when `skill.yaml` is absent.

## Related

- [ADR 0006](0006-license-split.md)
- [docs/write-a-skill.md](../docs/write-a-skill.md)
- `azerothl/Akasha_skills` (companion doc — link when published)
