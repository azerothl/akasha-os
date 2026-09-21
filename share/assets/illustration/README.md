# Illustration assets

Logical prefix: `/assets/illustration/**`  
Cap: `asset.read:/assets/illustration/**` (fail-closed; `..` denied)

## Packs

| Path | Contents |
|------|----------|
| `primitives/pack.yaml` | Core MeshBox / prefab library (ADR 0011 metres, Y-up) |

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
