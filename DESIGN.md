---
name: Akasha OS
description: Orrery in the ether — agents orbit, capabilities are the rods; cyan rails; hydrogen winding key only.
colors:
  void: "#070b14"
  signal: "#3ee0c4"
  hydrogen: "#e85d4c"
  paper: "#dce6f0"
  mute: "color-mix(in srgb, var(--signal) 48%, var(--void))"
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

**Creative North Star: "The Orrery"**

The public site is a mechanical orrery in night ether, not an AI-OS product hero and not a radio correlator. A live canvas draws MEMORY, CAPS, GPU, and AGENTS on elliptical rails; capability rods run from the hub to each globe. The landing fold is that machine on the left and an honesty stack on the right: Fira lede, one cyan swipe of HOST / NVIDIA rec. / NOT BOOTABLE, then cut-face Host / NVIDIA / Boot on void. The hydrogen winding key sits flush on the bottom rail. Inner pages are the same ether dimmed, with copy and cut-face stacks on void.

Personality is precise and technical. Honesty about Preview (host app, NVIDIA recommended, not bootable) sits in the same register as the thesis. Density is instrument-like: an 8px unit, flush square chrome, Archivo Narrow on the machine. Confirmed rejections: an AI-OS hero, a spectrogram correlator, a split-flap board, and a card grid of features. The header is the circular A-mark plus the words Akasha OS — never Station.

**Key Characteristics:**

- Night void field with a live canvas orrery (MEMORY, CAPS, GPU, AGENTS); plate weather dims the ether to 38% and tucks it to the corner
- Archivo Narrow 700 on rails, swipe, headlines, and the winding key; Archivo Narrow 600 on canvas orbit IDs
- Fira Sans 400 / 500 / 600 for lede, running copy, and honesty captions
- Hydrogen reserved for the Install / Download winding key
- Honesty as one cyan swipe plus centered void stacks; inner pages continue in cyan, not hydrogen
- Square bezels; integer unit `--u` (8px); two weathers (`body.sky` vs `body.plate`)

## Colors

Night void, cyan rails, and a single hydrogen winding key. Cool paper for type and plates; mute is mixed signal into void for idle chrome.

### Primary
- **Cyan Rail** (`signal`): The orrery’s phosphor. Orbit strokes, rods, hub, canvas labels, links, selection, caret, scrollbar thumb, focus rings, current nav lane, inner `h2`, honesty `dt`, continue links, code, and skip-link fill. Live chrome speaks cyan.

### Secondary
- **Hydrogen** (`hydrogen`): The winding key fill on Install (landing) and Download (install page) only. It is not a heading, link, hover, logo, or decorative accent.

### Neutral
- **Night Void** (`void`): Page ground, honesty column, skip-link inverse, theme-color, orrery clear. Paper type sits on it.
- **Cool Paper** (`paper`): Body type, plate fill, key type, hover inverse for links / key / continue.
- **Dim Rail** (`mute`): Idle nav lanes, language toggles, and footer. Mixed 48% signal into void.

### Named Rules
**The Hydrogen Key Rule.** Hydrogen fills the Install / Download winding key and nowhere else. Continue links, headings, the mark, and hover states stay cyan or paper.

**The Cyan Rail Rule.** If a control is idle, it sits on mute; if it is live, current, or a destination, it goes to signal — except the one hydrogen key.

## Typography

**Display Font:** Archivo Narrow (fallback Arial Narrow, sans-serif) at 600 and 700, self-hosted.
**Body Font:** Fira Sans (fallback Segoe UI, sans-serif) at 400, 400 italic, 500, and 600, self-hosted.

**Character:** Condensed machine lettering on the orrery and rails; humanist sans on plates and procedures. Archivo is uppercase on lanes, swipe, key, continue, and inner `h2`. The wordmark “Akasha OS” stays title case. Fira keeps the lede, plate captions, and running copy.

### Hierarchy
- **Display** (700, `clamp(1.8rem, 4vw, 2.6rem)`, line-height 0.9, tracking -0.02em, uppercase): Winding-key label (Install / Download). Continue labels use the same voice at `clamp(1.5rem, 3vw, 2.2rem)`.
- **Headline** (700, `clamp(1.85rem, 3.4vw, 2.7rem)`, line-height 1.12, tracking -0.02em): Inner-page `h1` and 404. Paper on void. The landing `h1` is visually hidden; the canvas is the machine.
- **Title** (700, 1.2rem, tracking 0.1em, uppercase, signal): Inner `h2` band labels (`hw.req`, `session GRANT`, chapter names).
- **Rail** (700, 1.05rem, tracking 0.08em, uppercase): Nav lanes, language toggles, swipe, honesty `dt`. Wordmark is 1.15rem / tracking 0.04em / 700, not uppercase. Key hint is 1.05rem / 0.08em.
- **Lede** (500, `clamp(1.15rem, 2vw, 1.45rem)`, line-height 1.35): Landing thesis and 404 line; max-width 36rem in the honesty column.
- **Body** (400, 1.0625rem, line-height 1.55): Running copy, history, transcripts. Measure 42rem. Honesty fact body is 0.98rem paper on void, centered, max 36rem. Honesty facts are `.fact` — never `.plate`, which is reserved for inner-page weather (`body.plate`).
- **Label** (700, 1.05rem, tracking 0.1em, uppercase, signal): Honesty Host / NVIDIA / Boot in Archivo Narrow. Canvas orbit IDs are Archivo Narrow 600 in signal. Footer is 0.92rem mute.

### Named Rules
**The Two Voices Rule.** Archivo Narrow speaks the machine (rails, swipe, titles, winding key, continue, Host / NVIDIA / Boot). Fira Sans speaks the log (lede, body, copy). Do not swap them.

**The Hidden Landing Title Rule.** On sky weather the `h1` is a screen-reader title only. Do not paint collision-scale type over the orrery.

## Layout

The site is a column: axis, then fold or plate-main, then footer. Inset is 3u (24px). The integer unit `--u` is 8px; all padding, gaps, and control heights are multiples of it.

Sky fold is a 2fr / 1fr grid (machine | honesty) with the winding key spanning the bottom row. The canvas is fixed full-viewport behind the shell. Honesty is a void column, centered: lede, swipe, Host / NVIDIA / Boot on void, repo link. Sky footer is hidden.

Plate-main is ordinary flow under the dimmed orrery. Grants and chapter lists may run to 52rem; copy, steps, and pre wrap at 42rem. Grants, ticks, and doc-bands stack with void gutters (2u–3u plates, 3u chapters, 8u between grants) — cut faces, not cards.

Below 820px the fold stacks, honesty comes first, the machine keeps 28dvh, orbit IDs appear as a four-item rail, and the sky key sticks to the bottom. Below 560px the key wraps so the hint takes a full row. Canvas orbit labels draw only when the canvas is at least 640px wide.

### Named Rules
**The Two Weathers Rule.** `body.sky` is the night fold (full-bleed orrery, hydrogen key, no footer). `body.plate` is working copy: ether at 38% opacity, hub shifted to the corner, continue links instead of hydrogen — except install, which keeps the Download key. Do not run the sky fold layout on plate pages.

**The Cut-Face Rule.** Honesty stacks, grants, ticks, and chapter bands are separated by void gaps. Do not wrap them in a card grid or sit them on paper sheets.

**The Measure Rule.** Running copy wraps at 42rem, left-aligned on inner pages. Do not center body copy with `margin: auto` outside `.honesty`.

## Elevation & Depth

Depth is the ether: the orrery recedes in void; sky honesty is an opaque void column beside it; plate weather drops the canvas to 38%. Surfaces are flush rectangles. There is no reusable shadow scale and no card lift. Focus is a 2px signal outline offset 0.5u (4px). Selection is signal fill with void type.

### Named Rules
**The Field Not Card Rule.** Do not lift content on drop shadows to imply cards. Do not sit honesty on paper sheets. The live orrery and tonal wells (8% signal mix under `pre`) carry depth.

## Shapes

Every control is a flush rectangle. No corner radius ships. Honesty stacks sit on void, not on paper sheets. The winding key is a full-width hydrogen rail (on plate-main it bleeds to the inset edges). The mark is a circular A inscribed in cyan rings on a void square — identity geometry, not a UI radius. The orrery itself is ellipses, rods, and globes; that geometry stays on the canvas.

### Named Rules
**The Square Bezel Rule.** `border-radius` stays 0 on chrome. Instruments are milled square. Do not round keys, lanes, plates, or bands. Circles belong to the mark and the orrery, not to buttons.

## Components

### Buttons
The winding key is the only primary control: hydrogen rail, Archivo Narrow 700 uppercase, key glyph, label, hint, min-height 10u. Hover and focus-visible invert to paper fill and void type, and wind the orrery (canvas speed target 3.4, lerp 0.08) unless `prefers-reduced-motion`. On sky it spans the fold; on install it bleeds the plate-main inset. There is no second hydrogen button.

Continue is the inner-page pager: cyan Archivo uppercase, no fill, 8u top margin. Hover goes to paper. 404 uses continue (Home), not the key.

### Chips
No filter chips. Language is a pair of rail buttons in the axis; the active language matches the current lane (void type on signal).

### Cards / Containers
Landing honesty uses cut-face stacks on void: Archivo Narrow signal titles, Fira paper body, centered, 3u gaps. Inner pages do not reuse sheets for grants or chapters — those are cut-face stacks on void. Linked ticks go to signal on hover. `pre` is signal type on an 8% signal-into-void well, Fira 500, 0.95rem. Ordered procedures use Fira body with Archivo Narrow 700 leading-zero counters in signal.

### Inputs / Fields
No text fields ship on the public site.

### Navigation
Axis: mark (40px A-mark + “Akasha OS”), wrapping lanes, EN/FR. Lanes are Archivo Narrow 700 uppercase, mute at rest, paper on hover, void-on-signal when `aria-current="page"`. Docs chapters are stacked doc-bands (signal index + paper name; hover to signal), not inverted wells. Footer is mute Fira at 0.92rem; links stay signal.

### Orrery
Fixed full-viewport canvas, pointer-events none. Four bodies on elliptical rails with rods to the hub: MEMORY, CAPS, GPU, AGENTS. Cyan strokes; paper / void / signal globes. Sky hub sits in the machine column; plate hub tucks upper-right at smaller span. Animates unless `prefers-reduced-motion: reduce`, which paints one seeded frame. Hovering or focusing `.key` winds the machine.

## Do's and Don'ts

### Do:
- **Do** let the live orrery fill the sky fold; keep the landing `h1` visually hidden.
- **Do** put HOST / NVIDIA rec. / NOT BOOTABLE in one cyan swipe, then Host / NVIDIA / Boot on void.
- **Do** keep hydrogen on the Install / Download winding key only; wind the orrery on key hover.
- **Do** use cyan continue links on inner pages and 404.
- **Do** set current nav and language as void type on a signal well.
- **Do** use `body.sky` for the landing (and 404) and `body.plate` for procedures.
- **Do** wrap running copy at 42rem, left-aligned; separate grants and chapters with void gutters.

### Don't:
- **Don't** build an AI-OS hero, a spectrogram correlator, a split-flap board, or a feature card grid.
- **Don't** label the header Station; the wordmark is Akasha OS.
- **Don't** use hydrogen for headings, links, continue, the mark, or hover states.
- **Don't** round corners or lift plates on card shadows.
- **Don't** put Archivo Narrow on running copy, or Fira on the winding key.
- **Don't** imply a bootable OS in chrome, imagery, or the swipe.
