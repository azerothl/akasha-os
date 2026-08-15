---
name: Akasha OS
description: Split-Flap Board — yellow destination type on matte black flaps, red GATE only.
colors:
  hall: "#0d0d0d"
  flap: "#1c1c1c"
  yellow: "#f5e31a"
  paper: "#f4f4f0"
  red: "#e23b16"
typography:
  headline:
    fontFamily: "Big Shoulders, Arial Narrow, sans-serif"
    fontSize: "clamp(2.6rem, 8vw, 5.6rem)"
    fontWeight: 700
    letterSpacing: "0.04em"
  body:
    fontFamily: "Big Shoulders Text, Arial Narrow, sans-serif"
    fontSize: "1.125rem"
    fontWeight: 400
    lineHeight: 1.5
---

# Design System: Akasha OS

## Overview

**Creative North Star: "Split-Flap Board"**

The public site is an airport departure board, not a product hero and not a UART log. Preview is the flight. Honesty rows (HOST, NVIDIA, NOT BOOTABLE) have already flipped. Install is the only red gate.

**Key Characteristics:**

- Self-hosted Big Shoulders (display) and Big Shoulders Text (body)
- Matte black flaps with a mechanical seam at mid-height
- Yellow destination type; paper for running copy; red reserved for GATE
- Horizon-wide rows, not a card grid
- One authored motion: flaps rotate in on the landing

## Named rules

**The Horizon Band Rule.** A flap owns the full width. Do not stack small cards.

**The Red Gate Rule.** Only INSTALL (or the page’s primary next destination) sits on red.

**The Phosphor Ban.** No CRT amber, scanlines, or mono-as-costume. This world replaced Host Console.

**The Measure Rule.** Running copy wraps at 72ch.
