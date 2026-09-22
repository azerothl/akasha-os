# Illustration assets

Logical prefix: `/assets/illustration/**`  
Cap: `asset.read:/assets/illustration/**` (fail-closed; `..` denied)

## Packs

| Path | Contents |
|------|----------|
| `primitives/pack.yaml` | Core MeshBox / prefab library (ADR 0011 metres, Y-up) |
| `styles/traditional-drawing/` | NPR style pack — Sketch / Pencil / Ink (YAML data) |

### Style pack (NPR)

| Id | Family | Notes |
|----|--------|-------|
| `sketch` | Sketch / Esquisse | Loose jittered line art |
| `pencil` | Pencil / Crayon | Graphite + cross-hatch (default DeclUI) |
| `ink` | Ink / Encre | Bold lines + sparse hatch |

Host embeds these styles in `aos-scene` for offline / CI. See `docs/illustration-npr-styles.md`.

### Primitives entries (v1 functional)

| Id | Kind | Notes |
|----|------|-------|
| `prop.box` | mesh_box | Unit-ish prop |
| `prop.ground` | mesh_box | Stage floor |
| `prop.pedestal` | mesh_box | Blockout plinth |
| `prop.counter` | mesh_box | Interior counter / desk |
| `prop.bookshelf` | mesh_box | Tall shelf mass |
| `prop.door` | mesh_box | Door slab |
| `prop.chair` | prefab | Seat + back + legs |
| `humanoid.placeholder` | prefab | Adult-scale box humanoid |
| `humanoid.slim` | prefab | Shorter / slimmer character variant |
| `scene.starter` | prefab | Ground + pedestal + box set |

Host loads the primitives pack from the embedded copy in `aos-scene` (offline). On-disk files under `share/assets/illustration/` are the source of truth for that embed.

Prompt → scene (`scene.compose`) picks from these entries via keyword heuristics (EN/FR).
