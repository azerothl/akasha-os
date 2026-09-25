# Illustration Studio — asset packs & marketplace hooks

**Status:** scaffolding (local catalogue offline; remote marketplace fail-closed)  
**Package:** `illustration-studio` **0.7.23** (MVP prefab pack §142 since 0.7.0; catalogue authoritative)  
**Related:** [caps](illustration-studio-caps.md), ADR 0011, ADR 0007 (module catalogue pattern — not a public store)

## What this is

Integration points for **Illustration asset / style / pose packs** so community packs can be indexed later **without** shipping a public marketplace in Preview.

| Surface | Behaviour |
|---------|-----------|
| Local catalogue | `share/assets/illustration/catalogue.yaml` (embedded in `aos-scene`) |
| `asset.pack.list` | List local packs — **offline**, needs `asset.read:/assets/illustration/**` |
| `asset.pack.describe` | Pack metadata + entry ids (asset kind) |
| `asset.marketplace.fetch` | DeclUI hook requiring `network.fetch` — host **always refuses** in Preview |
| `asset.instantiate` | Optional `pack_id` (default `illustration-primitives-v0`) resolved via catalogue |

## Security (fail-closed)

- Catalogue `network: deny` only (opt-in remote index rejected at load).
- Pack entries with `allows_scripts: true` or `allows_network: true` are rejected.
- Paths must stay under `/assets/illustration/` (`..` denied).
- Illustration Studio does **not** attest `network.fetch`.
- No unauthorized network: even with `network.fetch`, Preview fetch returns disabled.

## Pack kinds (hooks)

`asset` · `style` · `pose` — style/pose catalogue rows are reserved for sibling tracks; resolve loads **asset** packs only today.

## Paths

| Path | Role |
|------|------|
| `/assets/illustration/catalogue.yaml` | Local pack index |
| `/assets/illustration/primitives/pack.yaml` | Bundled primitives (`illustration-primitives-v0`) |

## Out of scope

Public store, paid packs, Discord distribution, auto-download, executables inside packs, NPR style YAML distribution (sibling **0.4.0** draft).
