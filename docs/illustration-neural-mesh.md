# Illustration Studio — neural mesh assist (foundation)

**Status:** Preview foundation (package `illustration-studio` **0.5.0**)  
**Spec:** draft §110–114 (AI 3D gen is **not** an MVP dependency)  
**Caps:** [`illustration-studio-caps.md`](illustration-studio-caps.md)

## Goal

Ship a fail-closed **host surface** for future neural / AI mesh assist without:

- a Blender-only editor path
- opaque model weights in-tree
- a second scene SoT beside SceneGraph

## Stub vs real

| Piece | Stub or real? |
|-------|----------------|
| Cap `mesh.neural` + DeclUI service `mesh.assist` | **Real** Preview host surface (fail-closed) |
| `backend=stub` procedural MeshBox assembly | **Real** offline path (deterministic; EN/FR keywords) |
| Proposal validation (part count / scale / bbox) | **Real** |
| SceneGraph insert (MeshBox under parent) | **Real** |
| DeclUI EN/FR Mesh assist section | **Real** |
| `backend=neural` weights / model pack | **Stub interface only** — returns `BackendUnavailable` |

## Flow

```text
prompt (+ backend)
  → mesh.assist (cap mesh.neural)
  → propose (stub | neural)
  → validate
  → insert MeshBox nodes into SceneGraph
  → wgpu edit view / RenderService beauty consume same SoT
```

## Keywords (stub)

| Kind | EN / FR cues (examples) |
|------|-------------------------|
| `crate` | crate, box, chest, caisse, coffre, boîte |
| `column` | column, pillar, colonne, pilier |
| `lamp` | lamp, lantern, lampe, lumière |
| `table` | table, desk, bureau, comptoir |
| `block` | fallback single MeshBox |

## Non-goals (this slice)

- Shipping GGUF / ONNX / diffusion 3D weights
- glTF arbitrary mesh import
- Blender sculpt / bpy mesh edit
- NPR style packs, agent co-edit locks, full IK (sibling tracks)
