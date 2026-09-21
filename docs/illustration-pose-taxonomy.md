# Experiment — Illustration pose taxonomy

**Status:** experiment / proposal  
**Branch:** `cursor/illustration-pose-taxonomy`  
**Base:** `cursor/illustration-surface` (`ef5ad3d`)  
**Language:** English | [Français](fr/illustration-pose-taxonomy.md)

## 1. Summary

This experiment explores a **third path** between:

1. the **SD multi-pass** pipeline (`illust.generate_image`) — staged image
   generation/editing where the agent directs and critiques but does not draw;
2. the current **vector** pipeline (`illust.compose` + recipes) — the agent
   (or thin templates) places geometry, often unreadable outside a few species.

The goal is an **agent that draws autonomously** on a **reliable armature**:
cascaded classification (family → group → type), a deterministic skeleton
(optionally refined by a small model), then agent dressing.

This document is the **living record** of the trial: decisions, non-goals,
contracts, success criteria, and an evolution journal.

## 2. Problem

| Approach | Strength | Weakness for “agent draws” |
|---|---|---|
| SD passes | Rich pixels | Not stroke drawing; agent only steers/reviews |
| Free vector (LLM) | Agent composes | Ellipse stacks; OOD subjects fail |
| Fixed recipes | Readable for ~4–5 species | No generality |

We want **pose control** without a flat closed catalog and without defaulting
to diffusion.

## 3. Goals

1. Classify a brief as **family → group → type** with **defaults** at each
   level when detail is unknown or low-confidence.
2. Emit a deterministic **skeleton** (`IllustrationSpec.skeleton` + contacts)
   from the chosen node.
3. Let the agent **dress** (volumes, contours, props) via `joint_bindings`
   without reinventing anatomy.
4. Keep an explicit **fallback** if even the family is uncertain (abstain →
   constrained free compose, or another pipeline).
5. Leave room to plug in later:
   - a **Jevlike Choice** model (`E:\rlcd_model`) for node selection;
   - a small **regression / sequence** head to refine joints.

## 4. Non-goals (first increment)

- Replacing or removing the SD pipeline.
- Training / wiring Jevlike into `aos-platformd` in the first commit.
- A production joint-regression model.
- Exhaustive biological taxonomy.
- Animation / sand-paper-found engines tied to this taxonomy.
- Automatic artistic acceptance (vision critic).

## 5. Target architecture

```text
brief (subject)
    │
    ▼
Cascade classifier (rules first; Jevlike later)
    family → group → type   (+ confidence / abstain)
    │
    ▼
Deterministic poser
    node → skeleton + contacts + family constraints
    │  (optional later: regression refines joints)
    ▼
Agent (compose / construction passes)
    path dressing bound to joints — must not replace skeleton
    │
    ▼
structural review → render_sheet → visual check → export
```

### 5.1 Roles

| Component | Produces | Does not produce |
|---|---|---|
| Taxonomy + classifier | node ids, confidence | pixels, decorative paths |
| Poser | `skeleton`, contacts, maybe base volumes | pencil/watercolor finish |
| Agent | contours, details, bound props | new joints outside the plan |
| Review | structural errors | artistic score |

### 5.2 Existing code hooks

Already in `crates/aos-proto/src/illustration.rs`:

- `IllustrationSkeletonJoint`, `joint_bindings`, contact IK;
- `human_pose_joints`, `ensure_construction_passes`;
- `construction_plan_for_subject` (semantic, not yet taxonomic).

Jevlike (`E:\rlcd_model`): Choice / Score / Noul — fits **bounded conditional
menus**, not continuous joint regression.

## 6. Taxonomy (draft v0)

At most three levels. Labels are **drawing-pragmatic**, not a full encyclopedia.

### 6.1 Family examples

| Id | Default skeleton |
|---|---|
| `humanoid` | biped, head, 2 arms, 2 legs |
| `mammal_quadruped` | horizontal body, 4 legs, head |
| `bird` | body, 2 legs, wing volumes |
| `reptile` | elongated body; limbs per group |
| `amphibian` | low quadruped-like default |
| `fish_bony` | fusiform + fins |
| `fish_cartilaginous` | fusiform + fins |
| `invertebrate` | minimal plan (segments / symmetry) |
| `object` | box/mass + supports |
| `nature` | plant/ground masses, no animal anatomy |
| `scene_prop` | support (bench, armchair, bike) |
| `unknown` | family abstention → fallback |

### 6.2 Example descents

```text
humanoid → primate → human
humanoid → primate → lemur
mammal_quadruped → carnivore → cat | dog | bear
invertebrate → arachnid → scorpion | spider
object → furniture → bench | armchair
nature → flora → tree | flower
```

If a deeper Choice abstains: **keep the parent default**.

### 6.3 Composite scenes

A brief like “man with a pipe in the garden” yields **several nodes**
(subject + props + décor). The main subject owns the animatable skeleton;
décor uses `nature` / `scene_prop`.

## 7. Decision policy

1. **Known + confident:** descend to type → pose template (+ later regression)
   → agent dresses.
2. **Known family, fuzzy detail:** stop at group/family defaults → agent dresses.
3. **Family abstention:** do **not** force a wrong template; documented
   fallback (strict free compose or explicit other pipeline — out of T1–T4
   code scope).
4. Forbidden: silently inventing an out-of-catalog type.

## 8. Minimal contracts

### 8.1 ClassificationResult (draft)

```text
family_id: string
group_id: string | null
type_id: string | null
confidence: 0..1
abstain: bool
secondary_nodes: [ClassificationResult]
```

### 8.2 PoseInstance (draft)

```text
node_id: string
skeleton: [IllustrationSkeletonJoint]
contacts: […]
joint_bindings_hints: { part_role → [joint_a, joint_b] }
pose_tags: [sitting, holding_pipe, …]
```

### 8.3 Agent rule

After a valid `PoseInstance`:

- `illust.compose` **keeps** `skeleton` / contacts;
- enrich must **not** replace a taxonomic skeleton with an opaque species recipe;
- dressing adds bound `path`s, not a new ellipse puppet.

## 9. Increment plan

| # | Deliverable | Status |
|---|----------|--------|
| T0 | Spec + journal (this document) | done |
| T1 | YAML/JSON catalog family→group→type + skeleton defaults (human, cat, object/support) | done |
| T2 | Keyword classifier → `ClassificationResult` | done |
| T3 | Poser: node → `skeleton` wired into enrich / construction passes | done |
| T4 | Proto tests + 3 prompts (gardener, cat, OOD) | done |
| T4.5 | Protect taxonomic skeleton; secondary poses; skeleton-aware review; compose merge | done |
| T5 | (Optional) Jevlike Choice dataset + eval outside Preview | later |
| T6 | (Optional) joint regression head | later |

## 10. Success criteria

**Pass if:**

- a gardener/pipe/garden brief yields a readable sitting human skeleton **without** SD;
- a cat brief yields a carnivore quadruped without the generic blob;
- an intentional OOD brief abstains or stays at family default without inventing a ghost type;
- the agent can add bound contours without wiping the skeleton;
- structural review rejects ellipse-only dressing when a taxonomic skeleton is present.

**Fail if:**

- the taxonomy blocks every unlisted subject;
- T4 still looks like an unreadable ellipse pile;
- SD is reintroduced as a silent “make it pretty” fallback.

## 11. Risks

| Risk | Mitigation |
|---|---|
| Catalog too large too soon | 3 levels; start with humanoid + mammal_quadruped + object + nature |
| Overconfident classifier | threshold + explicit abstain |
| Agent ignores skeleton | tool contract + tests + enrich that protects skeleton |
| Confusion with image pipeline | docs + separate vector path; no mix in T1–T4 |

## 12. Evolution journal

| Date | Event |
|---|---|
| 2026-09-20 | Branch `cursor/illustration-pose-taxonomy` created from `illustration-surface` (`ef5ad3d`). Decision: cascade taxonomy + deterministic poser + agent dressing; Jevlike / regression optional. Initial spec (T0). |
| 2026-09-20 | T1–T4 landed: `illustration_pose_taxonomy.json` + `illustration_taxonomy.rs` (classify / pose / apply); `enrich_illustration_puppet` fills empty skeleton before recipes; proto tests for gardener sitting, cat carnivore, OOD abstain; authored skeleton preserved. |
| 2026-09-20 | T4.5: taxonomy lock skips species/person recipe wipe; secondary joints (`pipe_`/`seat_`/`garden_`) + `hold_pipe` contact; review `skeleton_undressed`; compose merges prior skeleton/contacts/bindings; tool args pass skeleton fields. |

Later entries record catalog changes, visual test artifact paths, architecture
decisions, and abandonments.

## 13. References

- Base branch: `cursor/illustration-surface`
- Proto / skeleton: `crates/aos-proto/src/illustration.rs`
- Benchmarks: `benchmarks/illustration/README.md`
- Jevlike Choice model (outside Akasha tree): `E:\rlcd_model`
- Module state canvas: `illustration-module-state.canvas.tsx`
