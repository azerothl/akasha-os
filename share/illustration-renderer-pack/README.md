# Illustration Renderer Pack

Optional GPL redistributable for Blender beauty renders. **Not** part of the
Akasha Preview zip by default. Host `RenderService` talks to this pack across
a process + files boundary.

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

## Install (dev / manual)

1. Download an official Blender build from https://www.blender.org/download/
2. Either put the binary on `PATH` as `blender`, set `AOS_BLENDER_BIN`, or
   place it at `bin/blender` under this pack.
3. Point the host at the pack:
   ```bash
   export AOS_ILLUSTRATION_RENDERER_PACK=/path/to/share/illustration-renderer-pack
   export AOS_BLENDER_MODE=auto   # or require
   ```
4. From Illustration Studio DeclUI, use **Blender beauty** (`render.submit`
   backend=`blender`). Output: `/documents/illustrations/beauty-blender.png`.

## Without Blender (CI / offline)

```bash
export AOS_BLENDER_MODE=mock
cargo test -p aos-scene
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
