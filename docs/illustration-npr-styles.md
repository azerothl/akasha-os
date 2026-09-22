# Illustration Studio — NPR style packs

**Language:** English | Français  
**Status:** shipped (Sketch / Pencil / Ink) · 2026-09-22  
**Related:** [ADR 0011](adr/0011-scenegraph-numeric-conventions.md), [caps](illustration-studio-caps.md), [Renderer Pack](illustration-renderer-pack.md), [Blender backend](illustration-blender-backend.md)

## One-line rule

**Styles are data.** SceneGraph stays the only source of truth; NPR styles describe
beauty appearance and travel with `render.submit` — they never mutate the scene.

## Pack layout

```text
share/assets/illustration/styles/traditional-drawing/
├── manifest.yaml
├── sketch.yaml
├── pencil.yaml
└── ink.yaml
```

Logical prefix: `/assets/illustration/styles/**` (same `asset.read:/assets/illustration/**` cap).

Styles are embedded into `aos-scene` for offline / CI (no FS required).

## Families (Phase 5)

| Id | EN | FR | CPU approx | Blender mock / adapter |
|----|----|----|------------|------------------------|
| `sketch` | Sketch | Esquisse | Loose jittered lines, paper grain, no fill | Paper tint + Freestyle (when real) |
| `pencil` | Pencil | Crayon | Graphite lines + cross-hatch | Paper tint + Freestyle |
| `ink` | Ink | Encre | Bold black lines + sparse hatch | Paper tint + thicker Freestyle |

Aliases: `sketch_loose` / `esquisse`, `pencil_classic` / `crayon`, `ink_clean` / `encre`.

Unknown style ids **fail-closed** (`RenderError::UnknownStyle`). Omitting `style` keeps the legacy wireframe / stub look.

## RenderService wiring

`render.submit` accepts optional `style` or `style_id`:

```json
{
  "backend": "cpu",
  "pass": "beauty",
  "path": "/documents/illustrations/beauty-cpu.png",
  "scene_yaml": "...",
  "style": "pencil",
  "width": 320,
  "height": 240
}
```

| Backend | Behaviour |
|---------|-----------|
| `cpu` | Software NPR approximation (paper, jitter, hatch) |
| `stub` | Style-tinted solid (preview without geometry) |
| `blender` | Style in scene export JSON; mock differs by family; real adapter enables Freestyle |

Scene export field: `style: { id, family, line_width, jitter, … }` (GPL adapter only).

## DeclUI

Illustration Studio exposes an inline radio (EN/FR): Sketch / Pencil / Ink → `$local.style_id`
passed into Stub / CPU / Blender beauty actions.

## Style vs RenderPreset

| Concept | Owns |
|---------|------|
| **Style** | Appearance (line, hatch, paper) |
| **RenderPreset** | Production knobs (resolution, samples, quality) — not in this slice |

## Out of scope

Marketplace style distribution, watercolor / marker / charcoal packs, realtime NPR in the wgpu edit viewport.

---

## Français (résumé)

Les packs de style NPR sont des **données** YAML. Sketch / Pencil / Ink (Esquisse / Crayon / Encre)
passent par `render.submit`. Le CPU approxime ; Blender mock fonctionne sans binaire ; l’adaptateur
GPL Freestyle reste dans le Renderer Pack. La SceneGraph reste la seule source de vérité.
