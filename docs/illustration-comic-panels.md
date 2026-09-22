# Illustration Studio — comic page / panel layouts

**Package:** `illustration-studio` **0.5.0**  
**Spec:** Akasha Illustration Studio draft §162 Comic mode  
**Caps:** `comic.layout`, `comic.render`

## What this is

Comic pages are a **layout of panels**. Each panel owns a SceneGraph snapshot (ADR 0011). Layout templates place normalized rects with margin/gutter; `comic.render` composites CPU (or stub) beauty passes into one page PNG under `/documents/illustrations/`.

SceneGraph remains the only SoT **inside** each panel. The comic YAML is layout metadata + panel scene YAML — not a second materials/lights system. wgpu stays the edit viewport for the active scene; comic composite is beauty-only.

## Layout templates

| Id | EN | FR aliases | Panels |
|----|----|------------|--------|
| `single` | Single | `plein`, `full` | 1 |
| `two_h` | Two across | `deux_h` | 2 |
| `two_v` | Two stacked | `deux_v` | 2 |
| `grid_2x2` | 2×2 grid | `grille`, `quad` | 4 |
| `strip_3` | 3-strip | `bande`, `bande_3` | 3 |

Applying a layout clones the source SceneGraph per panel and applies light camera orbit / dolly variants so panels are not identical frames.

## DeclUI

- Radio: layout template → `$local.comic_layout`
- **Apply layout** → `comic.layout` → `$local.comic`
- **Bind scene → panel 1** → rebind current `$local.scene` into `panel_1`
- **Render page** → `comic.render` → `/documents/illustrations/comic-page.png` preview

EN/FR labels for all comic chrome.

## Stack note

Open drafts [#296](https://github.com/azerothl/akasha-os/pull/296) (NPR) and [#297](https://github.com/azerothl/akasha-os/pull/297) (agent coedit) both target package **0.4.0**. This track uses **0.5.0** to avoid colliding version stamps; merge order may require a catalogue/hash resign when stacking.

## Out of scope

Speech bubbles / Canvas lettering, multi-page storyboard timelines, NPR on panels (sibling), Blender child-process per-panel (in-process CPU composite; Blender backend id falls back to CPU).
