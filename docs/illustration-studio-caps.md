# Illustration Studio — host capabilities (product suite)

**Status:** product suite (RenderService + assets)  
**Related:** [ADR 0011](adr/0011-scenegraph-numeric-conventions.md), [Renderer Pack pointer](illustration-renderer-pack.md)

## Caps (fail-closed)

| Cap | Purpose |
|-----|---------|
| `fs.read:/documents/illustrations/**` | Read SceneGraph project YAML / UI state |
| `fs.write:/documents/illustrations/**` | Write project YAML / beauty PNG outputs |
| `render.stub` | Stub solid beauty backend (`render.submit` backend=`stub`, legacy `render.stub.beauty`) |
| `render.cpu` | CPU SceneGraph wireframe / beauty backend (`render.submit` backend=`cpu`) |
| `asset.read:/assets/illustration/**` | Read / instantiate Illustration asset packs |
| `tool.invoke:illustration-studio` | Invoke package tools |

Unknown DeclUI services remain rejected. Render writes also require the illustrations write cap. Asset instantiate requires `asset.read:/assets/illustration/**` (not ambient FS).

### Path rules

| Prefix | Rule |
|--------|------|
| `/documents/illustrations/**` | Render output paths must stay here (`..` denied) |
| `/assets/illustration/**` | Asset pack logical tree; `asset.read` fail-closed (optional future: `fs.read` under same prefix) |

## DeclUI services

| Service | Role |
|---------|------|
| `render.submit` | Job-shaped submit via host `RenderService` (backends: `stub`, `cpu`) |
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
| `/assets/illustration/primitives/pack.yaml` | Embedded primitives pack (`prop.box`, `prop.ground`, `prop.pedestal`, `humanoid.placeholder`, `scene.starter`) |

## Widgets

| Kind | Host behaviour |
|------|----------------|
| `scene3d` | Orbit / select / TRS — pointer-local (no per-move WASM) |
| `scene_tree` | Node list selection synced via local state |

## Out of scope here

Blender / GPL Renderer Pack, prompt→scene, IK/FK, NPR styles, wgpu viewport.
