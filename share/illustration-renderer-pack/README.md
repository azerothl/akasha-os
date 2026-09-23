# Illustration Renderer Pack

**Opt-in** GPL redistributable for Blender beauty renders. **Not** part of the
Akasha Preview zip by default. Host `RenderService` talks to this pack across
a process + files boundary. Without this pack, DeclUI **Blender beauty** is
**fail-closed** (unless `AOS_BLENDER_MODE=mock` for CI).

## Layout

```text
illustration-renderer-pack/
├── README.md                 (this file)
├── NOTICE
├── SOURCE-OFFER
├── LICENSES/
│   └── GPL-3.0-or-later.txt  (pointer / summary)
├── adapters/
│   └── akasha_beauty.py      (GPL-3.0-or-later — uses bpy)
└── bin/                      (optional) place Blender binary here
```

## Install / enable (dev / manual)

1. Keep or copy this tree (adapters + notices). Preview does **not** ship the
   Blender binary.
2. Download an official Blender build from Illustration Studio or
   https://www.blender.org/download/
3. Either put the binary on `PATH` as `blender`, set `AOS_BLENDER_BIN`, or
   place it at `bin/blender` under this pack.
4. Point the host at the pack (required when not using checkout defaults):
   ```bash
   export AOS_ILLUSTRATION_RENDERER_PACK=/path/to/share/illustration-renderer-pack
   export AOS_BLENDER_MODE=auto   # or require
   ```
5. In Illustration Studio DeclUI Beauty:
   - **Refresh Blender pack status** → should show mock-ready or ready
   - **Blender beauty** (`render.submit` backend=`blender`)
   - Output: `/documents/illustrations/beauty-blender.png`

For `MeshAsset` GLBs, the host validates and copies each file into the render
job. The adapter imports the GLB with Blender's glTF importer, retaining its
node hierarchy, textures and PBR materials. The viewport and CPU paths show
the same geometry and a simplified material base color.

## Modes

| `AOS_BLENDER_MODE` | Behaviour |
|--------------------|-----------|
| `auto` (default) | Pack + binary → real spawn (errors on failure, **no** silent mock); pack only → obvious mock; **pack missing → fail-closed** |
| `mock` | Deterministic teal/digest PNG — **no** Blender (CI) |
| `require` | Real spawn only; fail-closed if pack or binary missing |

Invalid `AOS_ILLUSTRATION_RENDERER_PACK` (set but not a directory) does **not**
fall through to checkout defaults — fail-closed.

## Without Blender binary (pack present)

```bash
export AOS_ILLUSTRATION_RENDERER_PACK=$PWD/share/illustration-renderer-pack
export AOS_BLENDER_MODE=auto   # or mock
cargo test -p aos-scene
# Adapter math (no bpy): identity camera must look +Y after Y-up→Z-up convert
python3 adapters/test_akasha_beauty_math.py
```

Mock mode never downloads or spawns Blender. It writes a deterministic teal
PNG from the SceneGraph export digest.

Optional real integration (local only):

```bash
export AOS_BLENDER_INTEGRATION=1
export AOS_BLENDER_MODE=require
export AOS_ILLUSTRATION_RENDERER_PACK=$PWD/share/illustration-renderer-pack
cargo test -p aos-scene -- --ignored integration_real_blender
```

## License boundary

- Host crates (`crates/**`) and guest modules (`modules/**`) stay free of
  Blender sources and `bpy`.
- This pack may contain Blender (GPL) + adapters (GPL-compatible).
- See `docs/illustration-renderer-pack.md` and ADR 0006.

## Isolation

See `docs/illustration-blender-backend.md` for the Win/Linux/macOS matrix and
P0-B GO gaps (AppContainer, sandbox-exec, ambient FS without bwrap).
