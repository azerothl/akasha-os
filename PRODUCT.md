# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Primary users are Preview cohort testers. They sit at a Windows 10/11 x64 or Linux x64 machine (NVIDIA GPU recommended; CPU-only supported but slower), deciding whether this install is worth their time. They do not want to clone the repo or use `cargo`. Their job is to understand why an agent-native OS is different from an agent app on a general-purpose OS, then install Preview and send feedback from the UI.

Other audiences (systems/security/AI researchers, commercial-license evaluators) exist in the repo but are not the primary user of this surface.

## Product Purpose

Akasha OS is an agent-native operating system: agents, models, tools, and memory are first-class system services with explicit capabilities, audit, and policy. Preview 0.5.0 is an installable host app on Windows/Linux (NVIDIA optional via CPU path), not a bootable OS image. A seL4 bare-metal track is separate.

Success for the public site: a tester understands the thesis (capabilities, semantic IPC, first-class GPU, offline-by-default), then downloads Preview and follows the tester protocol.

## Positioning

Most agent stacks are applications on top of a general-purpose OS. Akasha OS treats autonomy as a system problem: unforgeable revocable capabilities instead of ambient trust, typed intents on the system bus instead of POSIX as the agent’s native language, GPU placement and local GGUF inference as runtime concerns, and network egress deny-by-default.

A neighboring chat wrapper or “AI desktop” cannot truthfully claim that agents, models, and tools are kernel-adjacent services with capability tokens and a signed audit trail.

## Operating Context

- Public site: static pages in `website/`, published to GitHub Pages at https://azerothl.github.io/akasha-os/
- Download: GitHub Releases zip/tar.gz; `install.ps1` (Windows) or `./install.sh` (Linux)
- First run: model download if needed, then in-app tutorial
- Tester protocol: `docs/TESTER.md` — install without cargo, exercise paths, feedback from the UI
- End-user manual lives on the public site (`website/docs/`). Repo specs, ADRs, and the seL4 track stay in `docs/` (French mirrors under `docs/fr/`).
- Contact for commercial license: loic.peaudecerf@proton.me

## Capabilities and Constraints

Confirmed Preview surfaces: parallel persisted chat sessions; long-term memory (remember / recall); human- and agent-authored notes and tasks (WASM modules); agent goal loop with skills, tools, scheduler, optional MCP; caps list/revoke; live model metrics; opt-in web search/fetch (offline by default); themes; local feedback report plus GitHub issue; non-destructive update overlays from GitHub Releases.

Constraints:

- This is not a bootable OS image yet. Future work must never imply otherwise.
- Preview 0.5.0: Windows/Linux x64; NVIDIA GPU recommended; CPU-only path available; ~4 GB disk. No macOS.
- Dual licensing: AGPL-3.0-only and a commercial license (attribution + royalty). The Akasha OS trademark is reserved.
- Site and product copy are bilingual EN / FR with equivalent content.
- Stack for the public site is already decided: static HTML / CSS / JS in `website/`.

Site routes: landing plus grant, why, install, about, and an end-user docs hub (`website/docs/`). Repo specs stay on GitHub.

## Brand Commitments

- Name: **Akasha OS**. Trademark reserved.
- Voice in existing copy: precise, technical, no hype; states limits (not bootable, NVIDIA-only) in the same breath as the thesis.
- Binding languages: English and French.
- Identity constraint volunteered by the product itself: honesty about Preview vs the long-term seL4 track.

## Evidence on Hand

Real, usable:

- Product copy and structure in `website/index.html`, `website/styles.css`, `website/app.js`
- README, INSTALL, FIRST-RUN, TESTER, STATUS, vision, functional and technical specs (EN + `docs/fr/`)
- GitHub repository and Releases
- In-app tutorial and tester protocol (not screenshots committed as marketing assets)

Must not fabricate: testimonials, named customers, benchmark numbers, press quotes, “production” screenshots, user counts, or claims that Preview is a bootable OS.

## Product Principles

1. Thesis before download — a tester must grasp why this is an OS for agents, not another chat app, before the install CTA.
2. Honesty is a feature — Preview limits (host app, NVIDIA, not bootable) stay visible; they are not fine print.
3. Bounded autonomy — capabilities, audit, and deny-by-default network are the product story, not decoration.
4. Same facts in EN and FR — no language gets a softer or more complete claim.
5. Proof over theater — only ship claims the repo, releases, or a real run can back.

## Accessibility & Inclusion

No product-specific standard was set beyond bilingual EN/FR and the existing skip-link / semantic structure on the landing page. WCAG target is undecided.
