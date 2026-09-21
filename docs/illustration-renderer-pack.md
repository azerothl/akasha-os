# Illustration Renderer Pack — license boundary (pointer)

**Language:** English | Français (follow-up)

> Status: design note (P0-C) + isolation ship · 21/09/2026  
> Durable conventions: [ADR 0011](adr/0011-scenegraph-numeric-conventions.md)  
> Host vs guest licenses: [ADR 0006](../adr/0006-license-split.md)  
> Ops / isolation: [illustration-blender-backend.md](illustration-blender-backend.md)

## One-line rule

**Blender binaries and any `bpy` scripts stay in a separate GPL (or
GPL-compatible) Renderer Pack.** They must not ship inside Apache guest
modules (`modules/**`, `.aospkg`) or be linked into the AGPL host
(`crates/**`).

## Layers

| Layer | License | May contain Blender / `bpy`? |
|-------|---------|------------------------------|
| Host OS (`crates/`, daemons, UI, RenderService **client**) | AGPL-3.0-only + commercial | **No** Blender code or `bpy` |
| Guest modules (e.g. `illustration-studio.aospkg`) | Apache-2.0 | **No** |
| SceneGraph / project YAML / style **data** | Project / pack license (not Blender) | N/A — data only |
| **Illustration Renderer Pack** (optional install) | GPL for Blender binary; GPL-compatible for `bpy` adapters / NPR scripts | **Yes** |

Akasha talks to the pack across a **process + files/sockets** boundary
(Scene/Render package in → images out), analogous in spirit to allowlisted
`harness.run` spawns — not `shell.exec`, not in-process `bpy`.

Tree in-repo (scripts + notices only; **no** Blender binary in git):

`share/illustration-renderer-pack/`

User illustrations produced by Blender remain the user’s works (Blender FAQ:
program output), not GPL by virtue of rendering.

## Full inventory and redistribution checklist

See the project store note (P0-C): dependency inventory, source-offer,
notices, and lawyer review before any public redistribution of a pack that
includes Blender. This repo file is the durable **pointer** only.

## Related

- [ADR 0011 — SceneGraph numeric conventions](adr/0011-scenegraph-numeric-conventions.md)
- [ADR 0006 — license split](../adr/0006-license-split.md)
- [NOTICE](../NOTICE) — host vs guest SPDX layers
- [harness.md](harness.md) — allowlisted external process pattern (analogy, not a Blender runner)
- [illustration-blender-backend.md](illustration-blender-backend.md) — isolation matrix + mock CI path
