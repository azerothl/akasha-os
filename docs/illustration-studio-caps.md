# Illustration Studio — host capabilities (product suite)

**Status:** product suite (RenderService + assets + optional Blender isolation + wgpu edit viewport)  
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
| `tool.invoke:illustration-studio` | Invoke package tools |

Unknown DeclUI services remain rejected. Render writes also require the illustrations write cap. Asset instantiate requires `asset.read:/assets/illustration/**` (not ambient FS). Blender path is fail-closed without `render.blender`.

### Path rules

| Prefix | Rule |
|--------|------|
| `/documents/illustrations/**` | Render output paths must stay here (`..` denied) |
| `/assets/illustration/**` | Asset pack logical tree; `asset.read` fail-closed (optional future: `fs.read` under same prefix) |

## DeclUI services

| Service | Role |
|---------|------|
| `render.submit` | Job-shaped submit via host `RenderService` (backends: `stub`, `cpu`, `blender`) |
| `render.status` | Poll job state |
| `render.result` | Fetch completed job metadata / path |
| `render.stub.beauty` | Legacy thin wrapper → stub backend |
| `asset.instantiate` | Instantiate pack entry into SceneGraph YAML |

## Paths

| Path | Role |
|------|------|
| `/documents/illustrations/project.scene.yaml` | SceneGraph project (ADR 0011) |
| `/documents/illustrations/state.json` | Package UI prefs |
| `/documents/illustrations/beauty-stub.png` | Stub beauty output |
| `/documents/illustrations/beauty-cpu.png` | CPU beauty / wireframe output |
| `/documents/illustrations/beauty-blender.png` | Blender / mock beauty output |
| `/assets/illustration/primitives/pack.yaml` | Embedded primitives pack (`prop.box`, `prop.ground`, `prop.pedestal`, `humanoid.placeholder`, `scene.starter`) |

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

NPR style packs beyond minimal beauty, AI prompt→scene, IK/FK, beauty-quality GPU path, marketplace.
