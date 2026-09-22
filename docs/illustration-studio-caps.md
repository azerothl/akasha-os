# Illustration Studio — host capabilities (agent co-edit + NPR)

**Status:** agent co-edit + NPR styles + neural mesh assist foundation on functional v1  
**Related:** [ADR 0011](adr/0011-scenegraph-numeric-conventions.md), [NPR styles](illustration-npr-styles.md), [Renderer Pack pointer](illustration-renderer-pack.md), [Blender backend](illustration-blender-backend.md), [Neural mesh](illustration-neural-mesh.md), store notes `illustration-studio-agent-coedit.md`

## Caps (fail-closed)

| Cap | Purpose |
|-----|---------|
| `fs.read:/documents/illustrations/**` | Read SceneGraph project YAML / UI state |
| `fs.write:/documents/illustrations/**` | Write project YAML / beauty PNG outputs |
| `render.stub` | Stub solid beauty backend (`render.submit` backend=`stub`, legacy `render.stub.beauty`) |
| `render.cpu` | CPU SceneGraph wireframe / beauty / NPR approx (`render.submit` backend=`cpu`) |
| `render.blender` | Isolated Blender beauty (Renderer Pack) or deterministic mock (`backend=`blender`) |
| `asset.read:/assets/illustration/**` | Read / instantiate Illustration asset + style packs |
| `scene.compose` | Heuristic prompt → SceneGraph compose (`scene.compose` service) |
| `scene.pose` | Pose / IK-lite on humanoid joints (`scene.pose` service) |
| `scene.edit` | Select / TRS / transactional `scene.apply` / `scene.instantiate` |
| `scene.lock` | Set / clear / list semantic locks |
| `mesh.neural` | Neural / AI mesh assist (`mesh.assist`; stub procedural is Preview default) |
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
| `scene.apply` | `scene.edit` (+ pose/compose/asset as needed per op) | **Transactional** batch — all-or-nothing rollback |
| `scene.lock` / `scene.unlock` / `scene.locks` | `scene.lock` | Semantic locks (node or subtree) |
| `scene.compose` | `scene.compose` + asset read | Prompt → scene (fails if locks present for agents) |
| `scene.pose` | `scene.pose` | Pose / IK-lite |
| `scene.instantiate` | `scene.edit` + asset read | Instantiate pack entry |
| `asset.instantiate` | asset read | DeclUI legacy alias (same pack) |
| `render.submit` | `render.*` + write | Job-shaped beauty via RenderService (`stub`, `cpu`, `blender`); optional `style` / `style_id` |
| `render.status` / `render.result` | `render.*` | Job poll / result metadata |
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

| Path | Role |
|------|------|
| `/documents/illustrations/project.scene.yaml` | SceneGraph project (ADR 0011) |
| `/documents/illustrations/state.json` | Package UI prefs |
| `/documents/illustrations/beauty-stub.png` | Stub beauty output |
| `/documents/illustrations/beauty-cpu.png` | CPU beauty / wireframe / NPR output |
| `/documents/illustrations/beauty-blender.png` | Blender / mock beauty output |
| `/assets/illustration/primitives/pack.yaml` | Embedded primitives pack (props + humanoid variants + starter) |
| `/assets/illustration/styles/traditional-drawing/` | NPR style pack (Sketch / Pencil / Ink) |

## DeclUI

| Kind | Host behaviour |
|------|----------------|
| `scene3d` | **wgpu edit viewport** — lit MeshBox solid + wire overlay from SceneGraph; orbit / select / TRS pointer-local (no per-move WASM). Approximate realtime — **not** RenderService beauty / NPR / Blender |
| `scene_tree` | Node list selection synced via local state; lock / unlock buttons operate on `$local.selected_id` |
| `radio` (style) | DeclUI Sketch / Pencil / Ink → `$local.style_id` into beauty actions |

SceneGraph (`aos-scene`, ADR 0011) remains the **only** source of truth. The viewport does not register a `render.*` backend and must not grow a second materials/lights scene system. Blender is beauty-only (isolated Renderer Pack), never an editor. Styles are data and do not mutate the SceneGraph.

## Out of scope

Marketplace style packs, watercolor / marker / charcoal, comic/storyboard, real neural weights / model pack download, complete IK solver, Discord, multi-agent locks, transient pointer editing locks, realtime NPR in wgpu viewport.
