# Illustration Studio — neural mesh assist (foundation + MeshAsset spike)

**Status:** Preview foundation (package `illustration-studio` **0.7.0**)  
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
| DeclUI EN/FR Mesh assist section + pack status | **Real** |
| `NodeKind::MeshAsset` + glTF/GLB load (CPU + wgpu) | **Real** (foundation) |
| Neural Mesh Model Pack ABI (`share/illustration-neural-mesh-pack/`) | **Real** resolve + spawn shape; **mock** uses fixture GLB |
| `backend=neural` TRELLIS.2 GGUF weights / trellis.cpp | **Not in git** — point `AOS_NEURAL_MESH_*` at offline install |
| Pack missing | **`BackendUnavailable`** (fail-closed) |

## Flow

```text
prompt (+ backend)
  → mesh.assist (cap mesh.neural)
  → propose (stub | neural pack)
  → validate
  → insert MeshBox parts OR MeshAsset (GLB uri) into SceneGraph
  → wgpu edit view / RenderService beauty consume same SoT
```

## Neural pack env

| Env | Role |
|-----|------|
| `AOS_NEURAL_MESH_PACK` | Pack root (default `share/illustration-neural-mesh-pack`) |
| `AOS_NEURAL_MESH_BIN` | `trellis-cli` / LocalAI binary |
| `AOS_NEURAL_MESH_WEIGHTS` | Multi-file GGUF directory (LocalAI-io / ilintar) |
| `AOS_NEURAL_MESH_MODE` | `auto` \| `mock` \| `require` |
| `AOS_NEURAL_MESH_FIXTURE` | Override fixture GLB |
| `AOS_NEURAL_MESH_TIMEOUT_SECS` | Spawn timeout (default 600) |

See `share/illustration-neural-mesh-pack/README.md`.

## Keywords (stub)

| Kind | EN / FR cues (examples) |
|------|-------------------------|
| `crate` | crate, box, chest, caisse, coffre, boîte |
| `column` | column, pillar, colonne, pilier |
| `lamp` | lamp, lantern, lampe, lumière |
| `table` | table, desk, bureau, comptoir |
| `block` | fallback single MeshBox |

## Non-goals (this slice)

- Shipping GGUF / ONNX / diffusion 3D weights in the Preview zip
- Full PBR texture path in wgpu
- Blender beauty importing GLB (export carries `mesh_uri`; adapter TBD)
- Replacing §142 prefab MeshBox pack expansion (still primary for demos)
