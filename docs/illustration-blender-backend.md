# Illustration Blender backend — isolation & ops

**Language:** English  
**Status:** implemented (opt-in Renderer Pack; mock when pack present / explicit mock)  
**Related:** [Renderer Pack](illustration-renderer-pack.md), [caps](illustration-studio-caps.md), ADR 0011, P0-B spike

## Architecture

```text
DeclUI ──render.submit(backend=blender)──► RenderService
                                              │
                         ┌────────────────────┴────────────────────┐
                         │ BlenderRenderBackend                    │
                         │  1. probe opt-in pack (fail-closed)     │
                         │  2. export SceneGraph → scene.json      │
                         │     (Akasha Y-up, ADR 0011)             │
                         │  3a. mock PNG  OR  3b. isolated spawn   │
                         └────────────────────┬────────────────────┘
                                              │ fixed argv (no shell)
                                              ▼
                         Illustration Renderer Pack (opt-in install)
                         blender -b --factory-startup --python adapters/akasha_beauty.py -- <workdir>
```

SceneGraph remains the **only** source of truth. Blender is a beauty backend,
not an editor. The interactive `scene3d` viewport is unchanged.

### Opt-in UX (Preview)

1. Install / point at the pack (`AOS_ILLUSTRATION_RENDERER_PACK`, optional
   `AOS_BLENDER_BIN`) — see `share/illustration-renderer-pack/README.md`.
2. In DeclUI Beauty: **Refresh Blender pack status** (`render.pack.status`) shows
   EN/FR readiness (missing / mock-ready / binary ready).
3. **Blender beauty** (`render.submit` backend=`blender`) is **fail-closed** when
   the pack is absent. With pack + adapter but no binary, Auto uses deterministic
   mock. Explicit `AOS_BLENDER_MODE=mock` is the CI path (no pack required).
4. Cap `render.blender` remains fail-closed for both beauty and pack status.

### Y-up → Z-up (adapter contract)

Host export stays Akasha **Y-up RH** (`conventions: y_up_rh`). The pack adapter must:

1. Remap positions `(x, y, z) → (x, -z, y)` (≡ +90° about X).
2. Bake orientation with **`q_blender = q_basis(+90° X) * q_akasha`** — not a
   component shuffle. Identity cameras look **−Z** in Akasha; after remap that
   forward is Blender **+Y**. Leaving identity looking −Z aimed past the scene
   and produced NPR paper-only (white) beauty frames.
3. Prefer composed **world** TRS (bake parenting) before convert; aim the active
   camera at the mesh centroid for parity with host CPU `look_at_rh`.

## Isolation model

| Control | Behaviour |
|---------|-----------|
| Argv | Fixed: `-b --factory-startup --python <adapter> -- <workdir>` — no libre argv from modules |
| Shell | Never — no `shell.exec` |
| Workdir | Temp job dir with only `scene.json` (+ written `beauty.png`) |
| Env | `env_clear` + minimal `PATH` / `LANG` / `PYTHONNOUSERSITE` |
| Stdin | Closed |
| Timeout | `AOS_BLENDER_TIMEOUT_SECS` (default 120); kill on timeout |
| Linux `bwrap` | When available: `--unshare-net` + bind workdir / binary / adapter only |
| Windows | `CREATE_NO_WINDOW`; AppContainer FS+net **not** wired (P0-B gap) |
| macOS | Spawn only; sandbox-exec / Metal headless **not** wired (P0-B gap) |

### Platform matrix (from P0-B + this ship)

| Platform / mode | FS deny | Net deny | Status |
|-----------------|---------|----------|--------|
| Linux + `bwrap` | best-effort binds | `--unshare-net` | **GO** (best-effort) |
| Linux no `bwrap` | convention only | not enforced | **GO gap** |
| Windows | not enforced | not enforced | **GO gap** |
| macOS | not enforced | not enforced | **GO gap / high risk** |
| `AOS_BLENDER_MODE=mock` | n/a | n/a | **GO** (CI default path) |

## Caps

`render.blender` is required (fail-closed) for `backend=blender`. Write path still needs `fs.write:/documents/illustrations/**`.

## Env knobs

| Variable | Meaning |
|----------|---------|
| `AOS_BLENDER_MODE` | `auto` (default) · `mock` · `require` |
| `AOS_BLENDER_BIN` | Absolute path to Blender executable |
| `AOS_ILLUSTRATION_RENDERER_PACK` | Pack root (adapters + optional `bin/`). Invalid path → fail-closed (no checkout fallback) |
| `AOS_BLENDER_TIMEOUT_SECS` | Child wall clock (default 120) |
| `AOS_BLENDER_INTEGRATION=1` | Enable ignored real-Blender test |

### Mode behaviour

| Mode | Pack absent | Pack + adapter, no binary | Pack + binary |
|------|-------------|---------------------------|---------------|
| `auto` | **Fail-closed** | Mock PNG | Real spawn (fallback mock on spawn failure) |
| `mock` | Mock PNG (CI) | Mock PNG | Mock PNG |
| `require` | **Fail-closed** | **Fail-closed** | Real spawn |

## What works without Blender binary (pack present)

- Full `RenderService` ABI (`submit` / `status` / `result`) for backend `blender` via mock
- DeclUI **Blender beauty** button (Auto → mock) + **Refresh Blender pack status**
- Unit tests in `aos-scene` — **no Blender download in CI**
- SceneGraph → canonical `scene.json` export + digest golden stability

## Without the Renderer Pack

- Auto / Require → clear `backend unavailable` (fail-closed)
- DeclUI surfaces the error; pack status says **missing (opt-in)**
- CI still green with `AOS_BLENDER_MODE=mock`

## Manual / real Blender

Follow `share/illustration-renderer-pack/README.md`. On success, PNG lands at
`/documents/illustrations/beauty-blender.png`. If Blender or the pack is missing
under `AOS_BLENDER_MODE=require`, DeclUI surfaces a clear `backend unavailable` error.

## GPL boundary

`bpy` adapter lives only under `share/illustration-renderer-pack/`. Host crates
orchestrate process + files; they never `import bpy` or link Blender libs.
