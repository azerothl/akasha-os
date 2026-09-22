# Illustration Studio — storyboard / shot timeline

**Status:** storyboard sequencing track (spec §161)  
**Package:** `illustration-studio` **0.5.0** (avoids 0.4.0 clash with NPR #296 / agent coedit #297)  
**Related:** [caps](illustration-studio-caps.md), [ADR 0011](adr/0011-scenegraph-numeric-conventions.md)

## Caps (added)

| Cap | Purpose |
|-----|---------|
| `storyboard.edit` | Capture / apply / delete / reorder storyboard frames (fail-closed) |

## DeclUI services (added)

| Service | Role |
|---------|------|
| `storyboard.capture` | Snapshot camera + node transforms/visibility into a new ordered frame |
| `storyboard.apply` | Activate a frame (`index` / `frame_id` / `direction` prev\|next / `step`) onto SceneGraph |
| `storyboard.delete` | Remove active (or `frame_id`) frame; re-applies remaining active |
| `storyboard.move` | Reorder active frame (`direction`: earlier\|later) |

Existing compose / pose / render / asset services unchanged. Pose and asset instantiate preserve an embedded storyboard in project YAML.

## Project YAML

Optional `storyboard` block on `ProjectFile` (format_version stays **1** — backward compatible):

```yaml
storyboard:
  format_version: 1
  active_index: 0
  frames:
    - id: shot_01
      label: Wide rest
      duration_ms: 1200
      active_camera: camera
      transforms: { ... }
      visibility: { ... }
      cameras: { ... }
```

SceneGraph remains the only SoT for edit viewport and beauty. Frames are differential overrides (camera, poses, visibility, light placement via transforms).

## Out of scope

Comic page/panel layout (sibling track), animatics playback, NPR styles, agent coedit locks, deep IK.
