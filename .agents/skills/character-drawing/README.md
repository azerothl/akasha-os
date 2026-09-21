# Character Drawing Skill V1

Experimental skill for progressively constructing character drawings using traditional figure-drawing methodology.

The drawing follows:

```text
Gesture
   ↓
Skeleton
   ↓
Mannequin
   ↓
Construction
   ↓
Anatomy
   ↓
Clean Line
   ↓
Rendering
```

The fundamental principle is:

> Never redraw the character independently between passes.

Every pass continues the previous drawing.

---

# Architecture

The skill separates three responsibilities.

## Artistic knowledge

Stored in:

```text
references/
```

These documents explain:

- gesture
- proportions
- skeleton construction
- mannequin construction
- body construction
- anatomy
- line work

## Geometry

Stored in:

```text
scripts/
```

Geometry is deterministic whenever possible.

The LLM describes artistic intent.

The scripts resolve:

- proportions
- landmarks
- joints
- bones
- primitive volumes

## Image generation

Image generation happens progressively.

The output of one pass becomes the input of the next pass.

```text
CharacterSpec
     ↓
Resolved Geometry
     ↓
Gesture
     ↓
Skeleton
     ↓
Mannequin
     ↓
Construction
     ↓
Anatomy
     ↓
Clean Line
     ↓
Rendering
```

---

# Installation

Python 3.10+ recommended.

Create a virtual environment if desired.

```bash
python -m venv .venv
```

Windows:

```bash
.venv\Scripts\activate
```

Linux/macOS:

```bash
source .venv/bin/activate
```

Install dependencies:

```bash
pip install -r requirements.txt
```

---

# Quick test

From the skill directory:

```bash
python scripts/run.py examples/standing-character.yaml
```

The skill creates:

```text
output/
├── resolved-character.json
├── validation.json
├── drawing-state.json
│
├── 01_gesture.png
├── 02_skeleton.png
├── 03_mannequin.png
├── 04_construction.png
│
└── prompts/
    ├── gesture.txt
    ├── skeleton.txt
    ├── mannequin.txt
    ├── construction.txt
    ├── anatomy.txt
    ├── clean_line.txt
    └── rendering.txt
```

---

# CharacterSpec

The CharacterSpec describes artistic intent.

Example:

```yaml
character:
  species: human
  age_group: adult

  body:
    build: athletic

proportions:
  height_heads: 7.75
  shoulder_width_heads: 2.35

pose:
  type: standing

camera:
  projection: perspective
  view: three_quarter
```

CharacterSpec should NOT contain arbitrary joint coordinates.

The geometry engine calculates them.

---

# Resolved geometry

The geometry engine creates:

```text
resolved-character.json
```

This becomes the structural source of truth.

Example:

```json
{
  "joints": {
    "left_shoulder": {
      "x": 0.32,
      "y": 0.19
    },

    "left_elbow": {
      "x": 0.29,
      "y": 0.40
    }
  }
}
```

Once drawing begins, this geometry should be considered locked.

---

# DrawingState

`drawing-state.json` records the current drawing process.

It stores:

- current pass
- completed passes
- current image
- previous image
- existing strokes
- locked properties
- next pass

Example:

```json
{
  "current_pass": "mannequin",

  "completed_passes": [
    "gesture",
    "skeleton"
  ],

  "locked": {
    "camera": true,
    "proportions": true,
    "pose": true,
    "joints": true
  }
}
```

---

# Deterministic passes

V1 generates the first structural passes directly:

```text
01_gesture.png
02_skeleton.png
03_mannequin.png
04_construction.png
```

These passes all use the SAME geometry.

This prevents structural drift.

---

# Generative passes

Later stages can use an image generation model.

Recommended pipeline:

```text
04_construction.png
        │
        ▼
     anatomy
        │
        ▼
05_anatomy.png
        │
        ▼
    clean line
        │
        ▼
06_clean_line.png
        │
        ▼
    rendering
        │
        ▼
07_final.png
```

Every image transformation MUST use the previous image as visual input.

Never generate these images independently from text.

---

# Important rule for image models

Always include:

```text
Continue the supplied drawing.

Do not redraw the character from scratch.

Preserve exactly:

- pose
- camera
- framing
- proportions
- joints
- perspective
- body orientation

Add only the information required by the next drawing pass.
```

Pass-specific prompts are generated automatically in:

```text
output/prompts/
```

---

# Development roadmap

## V1

- CharacterSpec
- proportion engine
- deterministic 2D skeleton
- gesture rendering
- skeleton rendering
- mannequin rendering
- basic construction rendering
- DrawingState
- progressive image prompts

## V1.5

- arbitrary joint rotations
- pose presets
- asymmetric poses
- better line of action
- body-type parameters
- improved mannequin primitives
- hands and feet primitives
- construction overlay export

## V2

- 3D skeleton
- forward kinematics
- inverse kinematics
- joint constraints
- camera projection
- perspective
- foreshortening
- 3D primitive mannequin

## V3

- OpenPose adapter
- ControlNet adapter
- depth maps
- normal maps
- segmentation maps
- image-model adapters

## V4

- anatomical mass model
- clothing construction
- face construction
- hands construction
- character consistency
- style adapters
