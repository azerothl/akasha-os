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
├── adapters/
│   └── trellis_gguf.sh       (trellis.cpp argv; mock via AOS_NEURAL_MESH_ADAPTER_MOCK=1)
├── bin/                      (optional) place trellis-cli / LocalAI here
├── weights/                  (optional, out-of-tree) multi-file GGUF set
└── workdir/                  (host-written GLB outputs; gitignored locally)
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
export AOS_NEURAL_MESH_BIN=$AOS_NEURAL_MESH_PACK/adapters/trellis_gguf.sh   # or /path/to/trellis-cli
export AOS_NEURAL_MESH_WEIGHTS=/path/to/TRELLIS.2-4B-GGUF
export AOS_NEURAL_MESH_MODE=auto   # mock | require | auto
export AOS_NEURAL_MESH_TIMEOUT_SECS=600
export AOS_NEURAL_MESH_RES=512     # 512 | 1024 | 1536
```

Real spawn argv (trellis.cpp):

```text
trellis-cli <input.png> <output.glb> --models <GGUF_DIR> --res 512
```

## Modes

| `AOS_NEURAL_MESH_MODE` | Behaviour |
|------------------------|-----------|
| `mock` | Insert `fixtures/unit_cube.glb` as SceneGraph `MeshAsset` — **no** runner, **no** weights |
| `require` | Spawn fixed argv; needs pack + runner + GGUF weights dir + local `image_path`; fail-closed otherwise |
| `auto` | Prefer spawn when runner **and** weights ready (+ image); else fixture mock; pack missing → `BackendUnavailable` |

`ready_for_spawn` requires a weights directory that contains at least one `.gguf` (or a `.aos-weights-ready` marker for CI dry-runs).

## Isolation (Blender-pack pattern)

- Fixed argv only — never libre shell from DeclUI / WASM
- Workdir quarantine for image in + GLB out
- Optional Linux `bwrap --unshare-net`
- Env cleared of ambient secrets (allowlist: `AOS_NEURAL_MESH_ADAPTER_MOCK`, `FIXTURE`, `TRELLIS_CLI`, `PACK`)
- Windows: minimal `SystemRoot` / `System32` PATH + forwarded `VK_*` ICD discovery (optional `AOS_NEURAL_MESH_VK_DEVICE` to pin ggml device index; `AOS_NEURAL_MESH_VK_PREFER_DISCRETE=0` to disable NVIDIA ICD narrowing)
- Gaps: Windows AppContainer / macOS sandbox-exec (same class as Blender P0-B)

## Host contract

1. Cap `mesh.neural` + service `mesh.assist` `backend=neural`
2. Pack missing → `BackendUnavailable` (fail-closed)
3. Spawned GLB is validated (`load_gltf_mesh` poly/vertex caps) before insert
4. Validated GLB → `NodeKind::MeshAsset` + `mesh_uri` in SceneGraph (sole SoT)
5. wgpu edit view loads triangles; beauty CPU treats MeshAsset as box proxy for now
6. Conditioning image is a **local path** only (no URLs / no generate-time HF fetch)

## CI dry-run

```bash
export AOS_NEURAL_MESH_ADAPTER_MOCK=1
# adapter copies fixtures/unit_cube.glb → output (same argv as real trellis-cli)
cargo test -p aos-scene --lib -- neural_gguf_adapter_spawn
```

## License boundary

- Host crates and guest modules stay free of GGUF blobs and trellis/torch sources.
- This pack may contain runner binaries + GGUF weights installed by the user.
- See `NOTICE` and repo `docs/illustration-neural-mesh.md`.
