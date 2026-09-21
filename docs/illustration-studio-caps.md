# Illustration Studio — host capabilities (functional v1)

**Status:** functional v1 (prompt→scene + pose/IK-lite + richer assets + beauty backends + wgpu edit viewport)  
**Related:** [ADR 0011](adr/0011-scenegraph-numeric-conventions.md), [Renderer Pack pointer](illustration-renderer-pack.md), [Blender backend](illustration-blender-backend.md)

## Caps (fail-closed)

| Cap | Purpose |
|-----|---------|
| `fs.read:/documents/illustrations/**` | Read SceneGraph project YAML / UI state |
| `fs.write:/documents/illustrations/**` | Write project YAML / beauty PNG outputs |
| `render.stub` | Stub solid beauty backend (`render.submit` backend=`stub`, legacy `render.stub.beauty`) |
| `render.cpu` | CPU SceneGraph wireframe / beauty backend (`render.submit` backend=`cpu`) |
| `render.blender` | Isolated Blender beauty (Renderer Pack) or deterministic mock (`backend=`blender`) |
| `asset.read:/assets/illustration/**` | Read / instantiate Illustration asset packs |
| `scene.compose` | Heuristic prompt → SceneGraph compose (`scene.compose` service) |
| `scene.pose` | Pose / IK-lite on humanoid joints (`scene.pose` service) |
| `tool.invoke:illustration-studio` | Invoke package tools |

Unknown DeclUI services remain rejected. Render writes also require the illustrations write cap. Asset instantiate requires `asset.read:/assets/illustration/**` (not ambient FS). Compose requires `scene.compose` **and** `asset.read:/assets/illustration/**`. Pose requires `scene.pose`. Blender path is fail-closed without `render.blender`.

### Path rules

| Prefix | Rule |
|--------|------|
| `/documents/illustrations/**` | Render output paths must stay here (`..` denied) |
| `/assets/illustration/**` | Asset pack logical tree; `asset.read` fail-closed (optional future: `fs.read` under same prefix) |

## DeclUI services (agent / tool surface)

These host services are the **minimal agent/tool surface** for Illustration Studio. They mutate or replace SceneGraph YAML and return it; the guest persists via `illustration.project.save`. Caps are checked fail-closed before execution.

| Service | Cap(s) | Role |
|---------|--------|------|
| `scene.compose` | `scene.compose` + `asset.read:/assets/illustration/**` | Prompt → SceneGraph (templates / keyword heuristics; places camera + assets) |
| `scene.pose` | `scene.pose` | Apply pose preset / FK joint rotate / look-at (IK-lite); undoable ops in host |
| `render.submit` | `render.*` + write | Job-shaped beauty via RenderService (`stub`, `cpu`, `blender`) |
| `render.status` / `render.result` | `render.*` | Job poll / result metadata |
| `render.stub.beauty` | `render.stub` + write | Legacy thin wrapper → stub backend |
| `asset.instantiate` | `asset.read:/assets/illustration/**` | Instantiate pack entry into SceneGraph YAML |

### `scene.compose` input

```json
{ "prompt": "A man enters an old library." }
```

Result includes `scene_yaml`, `template_id`, `placed_assets`, `character_id`, `camera_id`.

### `scene.pose` input

```json
{
  "scene_yaml": "<project yaml>",
  "humanoid_root": "humanoid",
  "preset": "wave_right"
}
```

Alternates: `"look_at": { "x", "y", "z" }` or `"joint"` + `"axis"` + `"angle_rad"` for FK.

Presets: `rest`, `wave_right`, `wave_left`, `reach_forward`, `look_left`, `look_right`.

## Paths

| Path | Role |
|------|------|
| `/documents/illustrations/project.scene.yaml` | SceneGraph project (ADR 0011) |
| `/documents/illustrations/state.json` | Package UI prefs |
| `/documents/illustrations/beauty-stub.png` | Stub beauty output |
| `/documents/illustrations/beauty-cpu.png` | CPU beauty / wireframe output |
| `/documents/illustrations/beauty-blender.png` | Blender / mock beauty output |
| `/assets/illustration/primitives/pack.yaml` | Embedded primitives pack (props + humanoid variants + starter) |

## Widgets

| Kind | Host behaviour |
|------|----------------|
| `scene3d` | **wgpu edit viewport** — lit MeshBox solid + wire overlay from SceneGraph; orbit / select / TRS pointer-local (no per-move WASM). Approximate realtime — **not** RenderService beauty / NPR / Blender |
| `scene_tree` | Node list selection synced via local state |

### Viewport vs beauty (critical)

| Path | Role |
|------|------|
| DeclUI `scene3d` (`aos-scene::viewport`) | Interactive **edit view** of the same SceneGraph |
| `render.submit` / stub / cpu / blender | Offline **beauty** jobs via RenderService — separate ABI |

SceneGraph (`aos-scene`, ADR 0011) remains the **only** source of truth. The viewport does not register a `render.*` backend and must not grow a second materials/lights scene system. Blender is beauty-only (isolated Renderer Pack), never an editor.

## Out of scope here

Full NPR style packs, marketplace, comic/storyboard, neural mesh gen, complete IK solver, multi-agent locks.
