# Agent Workflow

This document describes how an agent should execute the Character Drawing skill.

---

# Phase 1 — Understand the request

Extract:

- character type
- age
- body type
- proportions
- pose
- action
- camera
- clothing
- style
- desired final stage

Do not start image generation yet.

---

# Phase 2 — Build CharacterSpec

Translate the request into:

```text
CharacterSpec
```

Use:

```text
templates/character-spec.yaml
```

Unknown artistic properties may use reasonable defaults.

Do NOT invent joint coordinates.

---

# Phase 3 — Read references

Load only relevant references.

Always read:

```text
references/gesture.md
references/proportions.md
references/skeleton.md
references/mannequin.md
references/construction.md
```

When required also read:

```text
references/anatomy.md
references/line-work.md
```

---

# Phase 4 — Resolve geometry

Run:

```bash
python scripts/run.py character-spec.yaml
```

Inspect:

```text
resolved-character.json
validation.json
```

If:

```json
"valid": false
```

do NOT continue.

Correct the CharacterSpec or geometry.

---

# Phase 5 — Establish structural drawing

Use deterministic output:

```text
01_gesture.png
02_skeleton.png
03_mannequin.png
04_construction.png
```

These images establish the authoritative structure.

The agent MUST NOT replace them with independently generated alternatives.

---

# Phase 6 — Anatomy pass

Input image:

```text
04_construction.png
```

Prompt:

```text
prompts/anatomy.txt
```

The image model should EDIT/CONTINUE the supplied image.

It should not create a new composition.

Save output as:

```text
05_anatomy.png
```

Update DrawingState.

---

# Phase 7 — Clean line

Input:

```text
05_anatomy.png
```

Prompt:

```text
prompts/clean_line.txt
```

Save:

```text
06_clean_line.png
```

Update DrawingState.

---

# Phase 8 — Rendering

Input:

```text
06_clean_line.png
```

Prompt:

```text
prompts/rendering.txt
```

Save:

```text
07_final.png
```

---

# Progressive construction mode

When the user wants to SEE the drawing evolve, preserve every pass.

Never return only the final image.

Keep:

```text
01_gesture.png
02_skeleton.png
03_mannequin.png
04_construction.png
05_anatomy.png
06_clean_line.png
07_final.png
```

---

# Critical consistency check

Before accepting a generated pass compare it against the previous pass.

Check:

- head location
- shoulders
- elbows
- wrists
- pelvis
- knees
- ankles
- silhouette orientation
- canvas framing

If significant structural drift occurred:

REJECT the generated pass.

Retry using the previous valid image.

Do NOT allow structural drift to propagate.

---

# Principle

The agent behaves like an artist working on one sheet of paper.

It does not behave like an image generator producing seven related pictures.

Each pass modifies the SAME drawing.
