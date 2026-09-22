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

### Primitives entries (§142 MVP pack)

#### Characters

| Id | Kind | Notes |
|----|------|-------|
| `humanoid.placeholder` | prefab | Adult male (Empty joints + MeshBox visuals) |
| `humanoid.female` | prefab | Adult female — same topology, softer proportions |
| `humanoid.slim` | prefab | Child / slim — shorter proportions |
| `quadruped.cat` | prefab | Cat (body, head, 4 two-bone legs, tail) |
| `quadruped.dog` | prefab | Dog (larger body, muzzle, legs, tail) |

#### Furniture

| Id | Kind | Notes |
|----|------|-------|
| `prop.table` | prefab | Table top + legs |
| `prop.chair` | prefab | Seat + back + legs |
| `prop.counter` | mesh_box | Interior counter |
| `prop.bookshelf` | mesh_box | Tall shelf mass |
| `prop.sofa` | prefab | Seat + back + arms |
| `prop.desk` | prefab | Desk top + drawer + legs |

#### Architecture

| Id | Kind | Notes |
|----|------|-------|
| `arch.wall` | mesh_box | Interior wall slab |
| `prop.ground` | mesh_box | Floor / stage |
| `prop.door` | mesh_box | Door slab |
| `arch.window` | prefab | Frame + pane + mullions |
| `arch.stairs` | prefab | Four-step blockout |

#### Props

| Id | Kind | Notes |
|----|------|-------|
| `prop.book` | mesh_box | Closed book |
| `prop.lamp` | prefab | Base + stem + shade |
| `prop.cup` | mesh_box | Small cup / mug |
| `prop.box` | mesh_box | Unit-ish prop |
| `prop.plant` | prefab | Pot + foliage |
| `prop.pedestal` | mesh_box | Blockout plinth |
| `scene.starter` | prefab | Ground + pedestal + box set |

Host loads the primitives pack from the embedded copy in `aos-scene` (offline). On-disk files under `share/assets/illustration/` are the source of truth for that embed.

Prompt → scene (`scene.compose`) picks from these entries via keyword heuristics (EN/FR): bookstore/library → walls, window, shelves, counter, sofa, lamp, books, plant, door; woman/femme → `humanoid.female`; dog/chien → `quadruped.dog`; cat/chat → `quadruped.cat`. Pose / IK uses Empty joint nodes so parent scale does not skew bone lengths.
