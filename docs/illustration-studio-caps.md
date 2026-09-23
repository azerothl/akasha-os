# Illustration Studio — host capabilities (IK/FK poses + agent co-edit + NPR)

**Status:** marketplace hooks + storyboard + comic panels + articulated IK/FK + pose library + neural mesh assist on co-edit/NPR + **MVP prefab pack §142 (0.7.0) + MeshAsset/TRELLIS spike (0.7.1) + edit chrome TRS/autosave/undo (0.7.2) + lights v0 (0.7.3) + TRELLIS.2 GGUF real runner (0.7.4) + Blender Renderer Pack opt-in / pack status (0.7.5) + Blender Auto fail-closed on spawn (0.7.6)**
**Related:** [ADR 0011](adr/0011-scenegraph-numeric-conventions.md), [NPR styles](illustration-npr-styles.md), [Renderer Pack pointer](illustration-renderer-pack.md), [Blender backend](illustration-blender-backend.md), [Neural mesh](illustration-neural-mesh.md), [Comic panels](illustration-comic-panels.md), [Storyboard](illustration-studio-storyboard.md), [Asset packs](illustration-asset-packs.md), store notes `illustration-studio-ik-poses.md` / `illustration-studio-agent-coedit.md` / `illustration-studio-prefab-pack.md`

> **Preview honesty:** wgpu `scene3d` is an **edit view**, not RenderService beauty. Beauty = stub/CPU in-tree; Blender = mock unless opt-in **GPL Renderer Pack** + binary. Compose = keyword heuristics (not LLM SceneIntent). Neural mesh = stub default; weights opt-in. Marketplace = offline packs only. This is **not** a claim of full MVP §140–§145. Tester-facing summary: [FEATURES.md §4c](FEATURES.md#4c-illustration-studio-experimental-module).

## Caps (fail-closed)

| Cap | Purpose |
|-----|---------|
| `fs.read:/documents/illustrations/**` | Read SceneGraph project YAML / UI state |
| `fs.write:/documents/illustrations/**` | Write project YAML / beauty PNG outputs |
| `render.stub` | Stub solid beauty backend (`render.submit` backend=`stub`, legacy `render.stub.beauty`) |
| `render.cpu` | CPU SceneGraph wireframe / beauty / NPR approx (`render.submit` backend=`cpu`) |
| `render.blender` | Isolated Blender beauty (Renderer Pack) or deterministic mock (`backend=`blender`); also gates `render.pack.status` |
| `asset.read:/assets/illustration/**` | Read / instantiate Illustration asset + style packs |
| `scene.compose` | Heuristic prompt → SceneGraph compose (`scene.compose` service) |
| `scene.pose` | FK / look-at / two-bone IK / pose presets / undo (`scene.pose` service) |
| `scene.edit` | Select / TRS / camera / light / transactional `scene.apply` / `scene.instantiate` |
| `scene.lock` | Set / clear / list semantic locks |
| `mesh.neural` | Neural / AI mesh assist (`mesh.assist`, `mesh.pack.status`; stub procedural is Preview default; neural fail-closed without Model Pack) |
| `comic.layout` | Create / mutate comic page panel layouts |
| `comic.render` | Composite comic page beauty PNG |
| `storyboard.edit` | Capture / apply / delete / reorder storyboard frames |
| `asset.pack.list` / `asset.pack.describe` | Local pack catalogue (uses `asset.read`) |
| `asset.marketplace.fetch` | Remote fetch hook — requires `network.fetch` (not attested; fail-closed) |
| `tool.invoke:illustration-studio` | Invoke package tools (incl. `scene.*`) |

Unknown DeclUI services remain rejected. Render writes also require the illustrations write cap. Asset instantiate requires `asset.read:/assets/illustration/**` (not ambient FS). Compose requires `scene.compose` **and** `asset.read:/assets/illustration/**`. Pose requires `scene.pose`. Blender path is fail-closed without `render.blender`. Unknown NPR `style` / `style_id` values fail-closed. Agents mutate SceneGraph only through capability-gated `scene.*` host_calls (no free FS).

## Agent / host_call surface

| Prefix | Rule |
|--------|------|
| `/documents/illustrations/**` | Render output paths must stay here (`..` denied) |
| `/assets/illustration/**` | Asset + style pack logical tree; `asset.read` fail-closed |

WASM tools on `illustration-studio` forward to platform `host_call` (`scene_host`). Agent `module.invoke` routes `scene.*` → module `illustration-studio`.

| Tool / service | Cap(s) | Role |
|----------------|--------|------|
| `scene.get` | `scene.edit` or illustrations read | Snapshot: yaml + selection + locks |
| `scene.select` | `scene.edit` | Set selection |
| `scene.trs` | `scene.edit` | Set node TRS (ADR 0011) |
| `scene.camera` | `scene.edit` | Set active camera eye / look-at / orbit / FOV |
| `scene.light` | `scene.edit` | Add / edit Light nodes (type / intensity / color) |
| `scene.apply` | `scene.edit` (+ pose/compose/asset as needed per op) | **Transactional** batch — all-or-nothing rollback |
| `scene.lock` / `scene.unlock` / `scene.locks` | `scene.lock` | Semantic locks (node or subtree) |
| `scene.compose` | `scene.compose` + asset read | Prompt → scene (fails if locks present for agents) |
| `scene.pose` | `scene.pose` | Preset / FK / look-at / two-bone IK; host undo stack |
| `scene.instantiate` | `scene.edit` + asset read | Instantiate pack entry |
| `asset.instantiate` | asset read | DeclUI legacy alias (same pack) |
| `render.submit` | `render.*` + write | Job-shaped beauty via RenderService (`stub`, `cpu`, `blender`); optional `style` / `style_id` |
| `render.status` / `render.result` | `render.*` | Job poll / result metadata |
| `render.pack.status` | `render.blender` | Probe opt-in Illustration Renderer Pack (EN/FR summary; fail-closed beauty when pack absent) |
| `render.stub.beauty` | `render.stub` + write | Legacy thin wrapper → stub backend |

These host services are the **minimal agent/tool surface** for Illustration Studio. Caps are checked fail-closed before execution.

### `render.submit` style

```json
{
  "backend": "cpu",
  "pass": "beauty",
  "path": "/documents/illustrations/beauty-cpu.png",
  "scene_yaml": "<project yaml>",
  "style": "pencil",
  "width": 320,
  "height": 240
}
```

Ids: `sketch`, `pencil`, `ink` (aliases: `esquisse`, `pencil_classic` / `crayon`, `encre` / `ink_clean`). See [illustration-npr-styles.md](illustration-npr-styles.md).

### `scene.compose` input

```json
{ "prompt": "A man enters an old library." }
```

Result includes `scene_yaml`, `template_id`, `placed_assets`, `character_id`, `camera_id`.

### `scene.pose` input

```json
{ "scene_yaml": "<yaml>", "humanoid_root": "humanoid", "preset": "sitting" }
```

Alternates: `"look_at": {x,y,z}` · `"joint"` + `"axis"` + `"angle_rad"` · `"ik": { "chain", "target", "pole"? }` · `"undo": true`.

**Presets:** `rest`, `standing`, `wave_right`, `wave_left`, `reach_forward`, `pointing`, `sitting`, `lying`, `look_left`, `look_right`, `quad_sit`.

**IK chains:** `arm_l` / `arm_r` / `leg_l` / `leg_r` / `front_l` / `front_r` / `back_l` / `back_r`.

### `scene.apply` example

```json
{
  "scene_yaml": "<project yaml>",
  "ops": [
    { "op": "select", "id": "humanoid" },
    { "op": "trs", "id": "box", "translation": { "x": 1.0, "y": 0.675, "z": 0.0 } },
    { "op": "pose", "humanoid_root": "humanoid", "preset": "wave_right" }
  ]
}
```

On lock conflict or validation failure the pre-batch SceneGraph is restored.

## Semantic locks

Persisted in project YAML (`locks:` map). Agents are fail-closed; humans (DeclUI) may still edit. DeclUI `scene_tree` shows `[locked]` / `[pose-lock]` markers.

```yaml
locks:
  camera:
    semantic: true
  humanoid:
    pose: true
    scope: subtree
```

## Assets

| Id | Notes |
|----|-------|
| `humanoid.placeholder` / `humanoid.slim` | Articulated Empty joints + MeshBox visuals (HumanoidRig v1) |
| `quadruped.cat` | Simple quadruped (body, neck/head, 4 two-bone legs, tail) |
| props / `scene.starter` | Unchanged from functional v1 |
| `/assets/illustration/styles/traditional-drawing/` | NPR style pack (Sketch / Pencil / Ink) |

| Path | Role |
|------|------|
| `/documents/illustrations/project.scene.yaml` | SceneGraph project (ADR 0011) |
| `/documents/illustrations/state.json` | Package UI prefs |
| `/documents/illustrations/beauty-stub.png` | Stub beauty output |
| `/documents/illustrations/beauty-cpu.png` | CPU beauty / wireframe / NPR output |
| `/documents/illustrations/beauty-blender.png` | Blender / mock beauty output |
| `/assets/illustration/primitives/pack.yaml` | Embedded primitives pack (props + humanoid variants + starter) |

## DeclUI

| Kind | Host behaviour |
|------|----------------|
| `scene3d` | **wgpu edit viewport** — lit MeshBox / MeshAsset solid + wire overlay from posed SceneGraph; orbit / select / Move·Rotate·Scale gizmos + full numeric TRS + Undo/Redo (pointer-local, no per-move WASM). Approximate realtime — **not** RenderService beauty / NPR / Blender |
| `scene_tree` | Node list selection synced via local state; lock / unlock buttons operate on `$local.selected_id` |
| `undo_redo` | When `scene_key` + `canvas_id` point at a `scene3d` viewport — global SceneGraph Undo/Redo chrome (same host stack). Layer-canvas mode unchanged when `layers_key` is set |
| `radio` (style) | DeclUI Sketch / Pencil / Ink → `$local.style_id` into beauty actions |
| Beauty · Blender | Tip + **Refresh Blender pack status** (`render.pack.status`) — opt-in GPL Renderer Pack; beauty fail-closed when pack absent |

SceneGraph (`aos-scene`, ADR 0011) remains the **only** source of truth. The viewport does not register a `render.*` backend and must not grow a second materials/lights scene system. Blender is beauty-only (isolated Renderer Pack), never an editor. Styles are data and do not mutate the SceneGraph. Viewport and beauty consume the same posed TRS.

## Out of scope

Marketplace style packs, watercolor / marker / charcoal, real neural weights / model pack download, Discord, multi-agent locks, transient pointer editing locks, realtime NPR in wgpu viewport, fingers/face/expressions.
