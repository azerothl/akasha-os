---
name: Akasha OS
description: Cloud-chamber scroll field on the landing (Abyssal grammar); ice/signal/hydrogen ramps; plate pages keep cut-face docs chrome.
colors:
  void: "#070b14"
  ice: "#5ee7ff"
  signal: "#2ef0c8"
  hydrogen: "#ff5a48"
  paper: "#e8eef6"
  mute: "color-mix(in srgb, var(--signal) 42%, var(--void))"
  plate: "color-mix(in srgb, var(--paper) 7%, var(--void))"
  plate-edge: "color-mix(in srgb, var(--signal) 55%, var(--void))"
typography:
  display:
    fontFamily: "Archivo Narrow, Arial Narrow, sans-serif"
    fontSize: "clamp(1.8rem, 4vw, 2.6rem)"
    fontWeight: 700
    lineHeight: 0.9
    letterSpacing: "-0.02em"
  headline:
    fontFamily: "Archivo Narrow, Arial Narrow, sans-serif"
    fontSize: "clamp(1.85rem, 3.4vw, 2.7rem)"
    fontWeight: 700
    lineHeight: 1.12
    letterSpacing: "-0.02em"
  title:
    fontFamily: "Archivo Narrow, Arial Narrow, sans-serif"
    fontSize: "1.2rem"
    fontWeight: 700
    letterSpacing: "0.1em"
  rail:
    fontFamily: "Archivo Narrow, Arial Narrow, sans-serif"
    fontSize: "1.05rem"
    fontWeight: 700
    letterSpacing: "0.08em"
  lede:
    fontFamily: "Fira Sans, Segoe UI, sans-serif"
    fontSize: "clamp(1.15rem, 2vw, 1.45rem)"
    fontWeight: 500
    lineHeight: 1.35
  body:
    fontFamily: "Fira Sans, Segoe UI, sans-serif"
    fontSize: "1.0625rem"
    fontWeight: 400
    lineHeight: 1.55
  label:
    fontFamily: "Fira Sans, Segoe UI, sans-serif"
    fontSize: "0.82rem"
    fontWeight: 600
    letterSpacing: "0.08em"
rounded:
  none: "0"
spacing:
  u: "8px"
  inset: "24px"
  measure: "42rem"
components:
  key:
    backgroundColor: "{colors.hydrogen}"
    textColor: "{colors.paper}"
    typography: "{typography.display}"
    rounded: "{rounded.none}"
    padding: "16px 24px"
    height: "80px"
  key-hover:
    backgroundColor: "{colors.paper}"
    textColor: "{colors.void}"
    typography: "{typography.display}"
    rounded: "{rounded.none}"
    padding: "16px 24px"
    height: "80px"
  continue:
    backgroundColor: "transparent"
    textColor: "{colors.signal}"
    typography: "{typography.display}"
    rounded: "{rounded.none}"
    padding: "16px 0"
  continue-hover:
    backgroundColor: "transparent"
    textColor: "{colors.paper}"
    typography: "{typography.display}"
    rounded: "{rounded.none}"
    padding: "16px 0"
  nav-lane:
    backgroundColor: "transparent"
    textColor: "{colors.mute}"
    typography: "{typography.rail}"
    rounded: "{rounded.none}"
    padding: "0 12px"
    height: "40px"
  nav-lane-current:
    backgroundColor: "{colors.signal}"
    textColor: "{colors.void}"
    typography: "{typography.rail}"
    rounded: "{rounded.none}"
    padding: "0 12px"
    height: "40px"
  plate:
    backgroundColor: "{colors.void}"
    textColor: "{colors.paper}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "0"
  swipe:
    backgroundColor: "{colors.signal}"
    textColor: "{colors.void}"
    typography: "{typography.rail}"
    rounded: "{rounded.none}"
    padding: "12px 0"
  doc-band:
    backgroundColor: "transparent"
    textColor: "{colors.paper}"
    typography: "{typography.rail}"
    rounded: "{rounded.none}"
    padding: "0"
---

# Design System: Akasha OS

## Overview

**Creative North Star: "The Cloud Chamber"**

The landing (`body.experience`) is a scroll-driven particle field in night void — Abyssal sticky-pin grammar fused to Akasha product truth. One WebGL seed cloud is reshaped by force-field presets as the visitor descends through Thesis → Caps → Surfaces → Local → Bound → Path → Install. Fixed mono chrome (wordmark, section index, Trace readout, progress rail) tints from the live field accent. Display type is set in authored line breaks (Archivo Narrow), not flowed paragraphs. Inner pages (`body.plate`) stay cut-face docs on void with the prior axis / lanes chrome.

Personality is precise and technical. Honesty about Preview (host app, NVIDIA recommended, not bootable) sits in the first beat, not as fine print. Confirmed rejections: app-store hero with screenshot collage, feature card grids, and implying a bootable OS.

**Key Characteristics:**

- Landing: full-bleed additive point field; sticky 185vh beats; live `--accent` from the field hot stop
- Archivo Narrow 700 uppercase display on landing titles; mono micro-labels for chrome / stats / links
- Fira Sans for notes and plate running copy
- Hydrogen appears as the Bound-beat field hot and alarm accent — not as a second primary button language on the landing (Install is a mono underline link)
- Plate pages: integer `--u` (8px), square bezels, continue links, docs rail
- Two weathers: `body.experience` (landing chamber) vs `body.plate` (procedures / docs)

## Colors

Night void, cyan rails, and a single hydrogen winding key. Cool paper for type and plates; mute is mixed signal into void for idle chrome.

### Primary
- **Cyan Rail** (`signal`): The orrery’s phosphor. Orbit strokes, rods, hub, canvas labels, links, selection, caret, scrollbar thumb, focus rings, current nav lane, inner `h2`, honesty `dt`, continue links, code, and skip-link fill. Live chrome speaks cyan.

### Secondary
- **Hydrogen** (`hydrogen`): Bound-beat field highlight and alarm accent; Download winding key on the install page. It is not a heading, idle link, logo, or decorative hover on plate chrome.

### Neutral
- **Night Void** (`void`): Page ground, honesty column, skip-link inverse, theme-color, orrery clear. Paper type sits on it.
- Cool Paper (`paper`): Body type, plate fill, key type, hover inverse for links / key / continue. Slightly brighter (`#e8eef6`) for long-form readability.
- Dim Rail (`mute`): Idle nav lanes, language toggles, and footer. Mixed 42% signal into void.
- Plate (`plate` / `plate-edge`): Lifted section backgrounds and cyan-tinted edges for `.grant`, `.section-plate`, `.ticks`, and `.doc-band` so inner pages are not a flat void.

### Named Rules
**The Hydrogen Key Rule.** On plate weather, hydrogen fills the Download winding key and nowhere else. On the landing field, hydrogen may tint the Bound preset only. Continue links, headings, the mark, and idle hover states stay cyan, ice, or paper.

**The Cyan Rail Rule.** If a control is idle, it sits on mute; if it is live, current, or a destination, it goes to signal — except the one hydrogen key.

## Typography

**Display Font:** Archivo Narrow (fallback Arial Narrow, sans-serif) at 600 and 700, self-hosted.
**Body Font:** Fira Sans (fallback Segoe UI, sans-serif) at 400, 400 italic, 500, and 600, self-hosted.

**Character:** Condensed machine lettering on landing display and plate rails; mono micro-labels on experience chrome; humanist sans on notes and procedures. Archivo is uppercase on landing titles, lanes, key, continue, and inner `h2`. The experience wordmark is `(AKASHA OS)` in mono. Fira keeps notes and plate running copy.

### Hierarchy
- **Display** (700, `clamp(2.5rem, 7vw, 7.2rem)`, line-height 0.86, tracking -0.045em, uppercase): Landing set titles (authored line breaks). Plate winding-key / continue stay at the smaller prior scale.
- **Headline** (700, `clamp(1.85rem, 3.4vw, 2.7rem)`, line-height 1.12, tracking -0.02em): Inner-page `h1` and 404. Paper on void.
- **Title** (700, 1.2rem, tracking 0.1em, uppercase, signal): Inner `h2` band labels.
- **Rail / Mono** (10px–11.5px mono, tracking ~0.14–0.2em, uppercase): Experience chrome, section labels, stats, underline links. Plate nav lanes stay Archivo Narrow 700.
- **Body** (400, 13px on landing notes; 1.0625rem on plate): Running copy. Plate measure 42rem.
- **Label**: Experience stats in accent mono; plate honesty / footer mute as before.

### Named Rules
**The Two Voices Rule.** Archivo Narrow speaks display and plate machine chrome. Mono speaks experience instrumentation. Fira Sans speaks notes and log copy. Do not swap them.

**The Set Title Rule.** Landing display lines are authored breaks inside `.line` masks. Do not let a landing headline reflow under its reveal mask.

## Layout

Landing: fixed canvas + fixed corner chrome + sticky pin sections (`--section-h: 185vh`). Corner / left / centre / closer layouts borrow Abyssal panel grammar. Footer follows the last beat.

Plate pages: axis, then plate-main, then footer. Inset is 3u (24px). The integer unit `--u` is 8px. Grants and chapter lists may run to 52rem; copy wraps at 42rem. Cut faces, not cards.

Below 820px experience section nav hides; display scales down. Plate pages keep their prior stacking rules.

### Named Rules
**The Two Weathers Rule.** `body.experience` is the scroll chamber (full-bleed field, sticky beats, mono chrome). `body.plate` is working copy with axis/lanes and continue links — except install, which keeps the Download key. Do not run the experience pin layout on plate pages.

**The Cut-Face Rule.** Honesty on sky is one paper mill on void, not a card grid. Inner grants, ticks, and chapter bands stay cut-face on void. Do not sit inner-page copy on paper sheets.

**The Measure Rule.** Running copy wraps at 42rem, left-aligned on inner pages. Do not center body copy with `margin: auto` outside `.honesty`.

## Elevation & Depth

Depth is the field: additive particles in void, chrome lit by `--accent`, sticky copy floating above. Surfaces are flush. No card lift. Focus is a 1px accent outline. Selection uses a cool cyan wash.

### Named Rules
**The Field Not Card Rule.** Do not lift content on drop shadows. The landing is a chamber, not a card grid. Plate pages stay cut-face on void; tonal wells under `pre` remain for code.

## Shapes

Flush rectangles and hairline rules. No corner radius on chrome. Experience links are underline hairlines, not filled keys. The mark geometry (circular A) may still appear on plate pages; the landing wordmark is mono text `(AKASHA OS)`.

### Named Rules
**The Square Bezel Rule.** `border-radius` stays 0 on chrome. Do not round keys, lanes, plates, or bands.

## Components

### Buttons
Landing primary action is a mono underline `.link` (Get Preview / section CTAs). Install page keeps the hydrogen winding key. Continue remains the plate pager: cyan Archivo uppercase, no fill.

### Chips
No filter chips. Experience language is a mono EN/FR pair tinted by `--accent` when pressed. Plate language may still use void-on-signal wells.

### Cards / Containers
No cards on the landing. Inner pages stay cut-face stacks on void. `pre` is signal type on an 8% signal-into-void well.

### Inputs / Fields
No text fields ship on the public site.

### Navigation
Experience: fixed top-left wordmark + Trace; top-right section index + Docs; top progress rail. Plate: axis mark + lanes + EN/FR. Docs chapters remain stacked doc-bands.

### Field
Fixed full-viewport WebGL points (`website/field.js`). Scroll blends neighbouring presets (forces + three-stop ramp). Accent CSS variable follows the hot stop. `prefers-reduced-motion: reduce` paints a static frame and skips Lenis / reveals.

## Do's and Don'ts

### Do:
- **Do** let the particle field own the first viewport; brand wordmark is a hero-level signal in the chrome.
- **Do** state host-app / not-bootable honesty in the first beat.
- **Do** drive chrome accent from the live field palette.
- **Do** use mono underline links on the landing and cyan continue on plate pages.
- **Do** use `body.experience` for the landing and `body.plate` for procedures.
- **Do** keep EN/FR equivalent claims.

### Don't:
- **Don't** rebuild an app-store hero with inset screenshots or a feature card grid on the landing.
- **Don't** label the header Station; the wordmark is Akasha OS / `(AKASHA OS)`.
- **Don't** round corners or lift plates on card shadows.
- **Don't** put Archivo Narrow on running note copy, or Fira on set display titles.
- **Don't** imply a bootable OS in chrome, imagery, or the closer.
