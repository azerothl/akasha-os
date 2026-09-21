# ADR 0011: SceneGraph numeric conventions

**Language:** English | Français (follow-up)

> Date: 21/09/2026 · Status: **accepted** (P0-A — conventions freeze only)  
> Related: Illustration Studio P0 gates (spec §356); [ADR 0006](../../adr/0006-license-split.md); [ADR 0009](0009-rich-module-app-contract.md)

## Context

Illustration Studio (and any future `scene3d` consumer) needs a host-owned
**SceneGraph** as the source of truth: project files, viewport, rigs, assets,
agent edits, and render jobs all share one numeric contract.

The product draft freezes coordinates before any `aos-scene` crate or
`scene3d` widget exists (P0-A). Without that freeze, each backend (glTF,
Blender, future GPU engines) will leak its own handedness, units, and
quaternion layout into Core.

**This ADR does not** implement SceneGraph, `scene3d`, RenderService, or a
Blender pack. It only freezes conventions and golden-test intent so later
crates cannot disagree.

## Decision

### 1. Akasha owns the conventions; backends convert

> **SceneGraph conventions are Akasha’s, never the render backend’s.**

Adapters (e.g. Blender, glTF import/export) perform explicit
`Akasha → BackendTransform → backend` conversion. Blender Z-up, glTF
conventions, or engine-specific camera spaces must not appear in
`aos-scene` types, project YAML/JSON, or agent tool schemas.

### 2. Frozen numeric contract

| Domain | Frozen value |
|--------|----------------|
| Handedness | **Right-handed** |
| Up axis | **+Y** |
| Forward axis | **−Z** (camera looks down −Z in local space when rotation is identity) |
| Distance unit | **metre** |
| Angle (serialization / API) | **radian** (`f32`) |
| Angle (human UI) | **degree** (convert at UI boundary only) |
| Position | `vec3<f32>` — `[x, y, z]` |
| Rotation (canonical) | **unit quaternion** `f32`, storage order **`[x, y, z, w]`** |
| Euler (non-canonical) | optional interchange only; if present, XYZ intrinsic, radians; must round-trip via quaternion |
| Scale | `vec3<f32>` — `[x, y, z]`; default `[1, 1, 1]` |
| Matrices | **column-major** `mat4<f32>`; `M * v` with column vectors |
| Transform compose | **TRS**: `T * R * S` (scale, then rotate, then translate) |
| Parenting | child local transform relative to parent; world = `parent_world * local` |
| Camera focal length | **millimetre** |
| Camera sensor width | **millimetre** (horizontal) |
| Color working space | **linear** (scene / render buffers) |
| Color UI input | **sRGB** (convert at UI / texture import boundary) |
| Time | **second** |

YAML/JSON examples may show compact position lists; the schema still means
the table above. Identity rotation is `[0, 0, 0, 1]` (xyzw).

### 3. Camera conventions

| Field | Rule |
|-------|------|
| Local look | Identity camera looks along **−Z**, up **+Y**, right **+X** |
| Projection | Perspective uses focal length + sensor width → horizontal FOV; orthographic uses metre height |
| Clip planes | Near/far in metres; near > 0 |
| Active camera | Named node id in the SceneGraph; never an implicit backend camera |

### 4. Serialization notes

- Prefer quaternion rotation in persisted SceneGraph; reject non-finite and
  near-zero-length quaternions at load (normalize if length ≈ 1).
- Do not persist column-major matrices as the primary transform form;
  derive matrices in runtime.
- Asset packs that ship glTF convert **into** Akasha space on import;
  exports convert **out**. Round-trip tests are mandatory (see below).

### 5. Golden-test intent (required before `aos-scene` ships)

No `aos-scene` crate lands until automated golden tests cover at least:

| Case | Intent |
|------|--------|
| Identity transform | World = local; quaternion `[0,0,0,1]` |
| 90° rotations about X / Y / Z | Known world points; radian inputs |
| Parent + child | Nested TRS; world matrix composition |
| Camera forward vector | Identity and rotated cameras → −Z in world when expected |
| Rig rest pose | Humanoid/quad rest joints stable under load/save |
| IK target | One documented IK solve fixture (tolerance table) |
| glTF → Akasha → Blender (or stub backend) round-trip | Positions/orientation within documented ε after adapter pair |

Tolerances and fixture paths live with the crate when it appears; this ADR
only freezes **which** cases exist.

> **Implementation note (foundation MVP):** `crates/aos-scene` lands the
> SceneGraph + YAML project format and covers identity / axis rotations /
> parenting / camera forward / load-save golden tests. Rig rest pose, IK
> fixture, and glTF↔backend round-trips remain deferred.

## Consequences

- P0-A is closed: SceneGraph / `aos-scene` design must cite this ADR.
- Blender (Z-up) and other engines stay behind adapters; Core types stay
  Y-up right-handed.
- Illustration Studio module (Apache guest) and host Scene Runtime (AGPL)
  share this contract via documented ABI — not via vendoring Blender types
  (see Renderer Pack license note / P0-C).
- FR ADR mirror may follow later, matching ADR 0008 / 0009 practice.

## Out of scope

- Implementing `aos-scene`, `scene3d`, rig runtime, or RenderService
- Choosing Blender vs alternate renderer (P0-B)
- GPL packaging of a Renderer Pack (P0-C)
- Color management beyond linear working / sRGB UI (ACES etc. later)
- Animation curves, skinning math detail, or physics units beyond seconds
