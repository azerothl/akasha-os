# Illustration Studio — NPR style packs

**Language:** English | Français  
**Status:** implementation added (Sketch / Pencil / Ink / Comic-Manga) · pending release
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
├── ink.yaml
└── comic_manga.yaml
```

Logical prefix: `/assets/illustration/styles/**` (same `asset.read:/assets/illustration/**` cap).

Styles are embedded into `aos-scene` for offline / CI (no FS required).

## Families (Phase 5)

| Id | EN | FR | CPU approx | Blender mock / adapter |
|----|----|----|------------|------------------------|
| `sketch` | Sketch | Esquisse | Loose jittered lines, paper grain, no fill | Paper tint + Freestyle (when real) |
| `pencil` | Pencil | Crayon | Graphite lines + cross-hatch | Paper tint + Freestyle |
| `ink` | Ink | Encre | Bold black lines + sparse hatch | Paper tint + thicker Freestyle |
| `comic_manga` | Comic / Manga | Comic / Manga | Black contours + stepped color shading | Black Freestyle contours + four luminance bands; materials remain unchanged |

Aliases: `sketch_loose` / `esquisse`, `pencil_classic` / `crayon`, `ink_clean` / `encre`, `comic` / `manga` / `bd`.

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

Illustration Studio exposes an inline radio (EN/FR): Sketch / Pencil / Ink / Comic-Manga → `$local.style_id`
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

Les packs de style NPR sont des **données** YAML. Sketch / Pencil / Ink / Comic-Manga passent par
`render.submit`. Comic-Manga combine des contours Freestyle noirs avec quatre paliers de luminance
appliqués à l’image rendue ; les matériaux et la SceneGraph restent inchangés.
