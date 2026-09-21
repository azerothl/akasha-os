# Illustration Studio — host capabilities (foundation)

**Status:** foundation MVP  
**Related:** [ADR 0011](adr/0011-scenegraph-numeric-conventions.md), [Renderer Pack pointer](illustration-renderer-pack.md)

## Caps (fail-closed)

| Cap | Purpose |
|-----|---------|
| `fs.read:/documents/illustrations/**` | Read SceneGraph project YAML / UI state |
| `fs.write:/documents/illustrations/**` | Write project YAML / stub beauty PNG |
| `render.stub` | Host stub beauty-pass service (`render.stub.beauty`) — **not** Blender |
| `tool.invoke:illustration-studio` | Invoke package tools |

Unknown DeclUI services remain rejected. `render.stub.beauty` also requires the write cap above.

## Paths

| Path | Role |
|------|------|
| `/documents/illustrations/project.scene.yaml` | SceneGraph project (ADR 0011) |
| `/documents/illustrations/state.json` | Package UI prefs |
| `/documents/illustrations/beauty-stub.png` | Stub beauty placeholder |

## Widgets

| Kind | Host behaviour |
|------|----------------|
| `scene3d` | Orbit / select / TRS — pointer-local (no per-move WASM) |
| `scene_tree` | Node list selection synced via local state |

## Out of scope here

Blender / GPL Renderer Pack, prompt→scene, IK/FK, asset packs, NPR styles.
