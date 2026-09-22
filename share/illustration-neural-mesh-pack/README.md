# Illustration Neural Mesh Pack

Optional **TRELLIS.2 GGUF** Model Pack for Illustration Studio `mesh.assist`
(`backend=neural`). **Not** part of the Akasha Preview zip by default. Host
talks to this pack across a **process + files** boundary (same isolation shape
as the Blender Renderer Pack).

## Layout

```text
illustration-neural-mesh-pack/
├── README.md                 (this file)
├── NOTICE
├── SOURCE-OFFER
├── LICENSES/                 (MIT Microsoft + DINOv3 Meta pointers)
├── fixtures/
│   └── unit_cube.glb         (tiny CI / mock MeshAsset — NOT model output)
├── adapters/                 (optional CLI wrappers; no weights in git)
├── bin/                      (optional) place trellis-cli / LocalAI here
└── weights/                  (optional, out-of-tree) multi-file GGUF set
```

## Do **not** vendor GGUF weights into akasha-os

Full TRELLIS.2 shard sets are multi‑GB. Point the host at an offline copy:

| Source | Role |
|--------|------|
| [LocalAI-io/TRELLIS.2-4B-GGUF](https://huggingface.co/LocalAI-io/TRELLIS.2-4B-GGUF) | f16 stage files (MIT Microsoft) |
| [LocalAI-io/TRELLIS-image-large-GGUF](https://huggingface.co/LocalAI-io/TRELLIS-image-large-GGUF) | v1 `ss_dec` companion |
| [LocalAI-io/dinov3-vitl16-pretrain-lvd1689m-GGUF](https://huggingface.co/LocalAI-io/dinov3-vitl16-pretrain-lvd1689m-GGUF) | DINOv3 (Meta license — inventory in NOTICE) |
| [ilintar/trellis2-gguf](https://huggingface.co/ilintar/trellis2-gguf) | q4 / q8 full sets for trellis.cpp |
| [pwilkin/trellis.cpp](https://github.com/pwilkin/trellis.cpp) | `trellis-cli` / `trellis-server` (Win/Linux/Metal) |
| [LocalAI 3D docs](https://localai.io/docs/features/3d-generation/) | `local-ai run trellis2-4b-geometry` (~7 GB) |

```bash
# Example (dev machine — never at generate-time over the network in Preview)
export AOS_NEURAL_MESH_PACK=/path/to/share/illustration-neural-mesh-pack
export AOS_NEURAL_MESH_BIN=/path/to/trellis-cli
export AOS_NEURAL_MESH_WEIGHTS=/path/to/TRELLIS.2-4B-GGUF
export AOS_NEURAL_MESH_MODE=auto   # mock | require | auto
export AOS_NEURAL_MESH_TIMEOUT_SECS=600
```

## Modes

| `AOS_NEURAL_MESH_MODE` | Behaviour |
|------------------------|-----------|
| `mock` (CI default when pack present) | Insert `fixtures/unit_cube.glb` as SceneGraph `MeshAsset` — **no** runner, **no** weights |
| `require` | Spawn fixed argv (`trellis-cli --input … --output … [--weights …]`); fail-closed if missing |
| `auto` | Prefer spawn when runner exists; else fixture mock; pack missing → `BackendUnavailable` |

## Isolation (Blender-pack pattern)

- Fixed argv only — never libre shell from DeclUI / WASM
- Workdir quarantine for image in + GLB out
- Optional Linux `bwrap --unshare-net`
- Env cleared of ambient secrets
- Gaps: Windows AppContainer / macOS sandbox-exec (same class as Blender P0-B)

## Host contract

1. Cap `mesh.neural` + service `mesh.assist` `backend=neural`
2. Pack missing → `BackendUnavailable` (fail-closed)
3. Validated GLB → `NodeKind::MeshAsset` + `mesh_uri` in SceneGraph (sole SoT)
4. wgpu edit view loads triangles; beauty CPU treats MeshAsset as box proxy for now

## License boundary

- Host crates and guest modules stay free of GGUF blobs and trellis/torch sources.
- This pack may contain runner binaries + GGUF weights installed by the user.
- See `NOTICE` and repo `docs/illustration-neural-mesh.md`.
