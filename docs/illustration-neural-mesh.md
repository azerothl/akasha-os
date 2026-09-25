# Illustration Studio — neural mesh assist (TRELLIS.2 GGUF runner)

**Status:** Preview foundation + real spawn path (package `illustration-studio` **0.7.23**; TRELLIS.2 GGUF runner since 0.7.4)  
**Spec:** draft §110–114 (AI 3D gen is **not** an MVP dependency)  
**Caps:** [`illustration-studio-caps.md`](illustration-studio-caps.md)

## Goal

Ship a fail-closed **host surface** for neural / AI mesh assist without:

- a Blender-only editor path
- opaque model weights in-tree
- a second scene SoT beside SceneGraph
- Gradio / WebView UI

## Stub vs real

| Piece | Stub or real? |
|-------|----------------|
| Cap `mesh.neural` + DeclUI service `mesh.assist` | **Real** Preview host surface (fail-closed) |
| `backend=stub` procedural MeshBox assembly | **Real** offline path (deterministic; EN/FR keywords) |
| Proposal validation (part count / scale / bbox) | **Real** |
| SceneGraph insert (MeshBox under parent) | **Real** |
| DeclUI EN/FR Mesh assist + pack status + image path | **Real** |
| `NodeKind::MeshAsset` + glTF/GLB load (CPU + wgpu) | **Real** |
| Neural Mesh Model Pack ABI | **Real** resolve + spawn |
| `trellis-cli` argv (`<in> <out> --models <dir> --res N`) | **Real** (matches pwilkin/trellis.cpp) |
| Pack adapter `adapters/trellis_gguf.sh` | **Real** (wraps CLI; CI mock via `AOS_NEURAL_MESH_ADAPTER_MOCK=1`) |
| Spawned GLB validation before MeshAsset insert | **Real** |
| TRELLIS.2 GGUF weights / trellis.cpp binary | **Not in git** — point `AOS_NEURAL_MESH_*` at offline install |
| Pack missing / weights missing in `require` | **`BackendUnavailable`** (fail-closed) |

## Flow

```text
prompt (+ optional local image_path, backend)
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
| `AOS_NEURAL_MESH_BIN` | `trellis-cli` or `adapters/trellis_gguf.sh` |
| `AOS_NEURAL_MESH_WEIGHTS` | Multi-file GGUF directory (LocalAI-io / ilintar) |
| `AOS_NEURAL_MESH_MODE` | `auto` \| `mock` \| `require` |
| `AOS_NEURAL_MESH_FIXTURE` | Override fixture GLB |
| `AOS_NEURAL_MESH_TIMEOUT_SECS` | Spawn timeout (default 600) |
| `AOS_NEURAL_MESH_RES` | `--res` 512 / 1024 / 1536 (default 512) |
| `AOS_NEURAL_MESH_ADAPTER_MOCK` | `1` → adapter copies fixture (CI) |

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
- LocalAI long-lived HTTP server as the default Preview path (prefer argv CLI)
