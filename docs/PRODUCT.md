# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Primary users are Preview cohort testers. They sit at a Windows 10/11 x64, Linux x64, or macOS Apple Silicon machine (NVIDIA GPU recommended on Win/Linux; CPU-only supported but slower; Mac builds are unsigned), deciding whether this install is worth their time. They do not want to clone the repo or use `cargo`. Their job is to understand why an agent-native OS is different from an agent app on a general-purpose OS, then install Preview, walk the 15-minute path, and send feedback from the UI.

Other audiences (systems/security/AI researchers, commercial-license evaluators) exist in the repo but are not the primary user of this surface.

## Product Purpose

Akasha OS is an agent-native operating system: agents, models, tools, and memory are first-class system services with explicit capabilities, audit, and policy. Preview 0.15.1 is an installable host app on Windows/Linux (NVIDIA optional via CPU path in the same zip) and macOS Apple Silicon (unsigned), not a bootable OS image. A seL4 bare-metal track is separate.

Success for the public site: a tester understands, in full sentences, what Preview can do and how it differs from a chat wrapper; then installs and follows the 15-minute path. Cohort gate: 3 Windows + 1 Linux + 1 macOS Apple Silicon, each with a usable `var/feedback/` report.

## Positioning

Most agent stacks are applications on top of a general-purpose OS. Akasha OS treats autonomy as a system problem: unforgeable revocable capabilities instead of ambient trust, typed intents on the system bus instead of POSIX as the agent’s native language, GPU placement and local GGUF inference as runtime concerns, and network egress deny-by-default.

A neighboring chat wrapper or “AI desktop” cannot truthfully claim that agents, models, and tools are kernel-adjacent services with capability tokens and a signed audit trail.

## Operating Context

- Public site: static pages in `website/`, published to GitHub Pages at https://azerothl.github.io/akasha-os/
- Download: GitHub Releases zip/tar.gz; `install.ps1` (Windows) or `./install.sh` (Linux)
- First run: model download if needed, then in-app tutorial
- Tester protocol: `docs/TESTER.md` — 15-minute path (install without cargo, one offline chat, one note, feedback from the UI); long protocol remains the team checklist
- Community hangout: GitHub Discussions; public page `website/community.html`. No Discord yet.
- End-user manual lives on the public site (`website/docs/`). Repo specs, ADRs, and the seL4 track stay in `docs/` (French mirrors under `docs/fr/`).
- Contact for commercial license: loic.peaudecerf@proton.me

## Capabilities and Constraints

Confirmed Preview surfaces: parallel persisted chat sessions; long-term memory (remember / recall); human- and agent-authored notes and tasks (WASM modules); agent goal loop with skills, tools, scheduler, optional MCP; declarative module UI host; caps list/revoke; live model metrics; Image Studio and optional local image/TTS packs via Models; Providers tab (OpenAI-compat cloud + loopback); mid-token migrate; module uninstall (non-bundled); opt-in web search/fetch (offline by default); themes; local feedback report plus GitHub issue; non-destructive update overlays from GitHub Releases.

Constraints:

- This is not a bootable OS image yet. Future work must never imply otherwise.
- Preview 0.15.1: Windows/Linux x64; macOS Apple Silicon (unsigned); NVIDIA GPU recommended on Win/Linux; CPU-only path in the same Win/Linux artefact; ~8 GB disk recommended. No Intel Mac.
- Dual licensing: AGPL-3.0-only and a commercial license (attribution + royalty). The Akasha OS trademark is reserved.
- Site and product copy are bilingual EN / FR with equivalent content.
- Stack for the public site is already decided: static HTML / CSS / JS in `website/`.

Site routes: landing plus grant, why, install, about, community, an end-user docs hub (`website/docs/`), a ten-minute skill guide (`website/docs/skill.html`), a first-module guide (`website/docs/module.html`), and a version log (`website/docs/whats-new.html`). Repo specs stay on GitHub.

## Brand Commitments

- Name: **Akasha OS**. Trademark reserved.
- Voice: precise and technical, no hype. Prefer **full sentences and short paragraphs** over keyword stamps (`GRANT` / `DENY` as primary labels). Explain what the user can do in the UI and how Preview can be extended (modules, skills, MCP, Providers). State limits (host app, NVIDIA recommended, not bootable) in the same breath as the thesis, without sounding like a checklist of refusals.
- Visual identity: **cloud chamber** — ākāśa as the medium; agent actions ionize tracks. Canonical mark is the trace-A (hydrogen spark at the origin), not rings, not bindu. Palette: void `#070b14`, ice-track `#5ee7ff` (event lines), signal `#2ef0c8` (live chrome), hydrogen `#ff5a48` (spark / alarm only), paper `#e8eef6`. Densities: mark / lockup / labeled tracks (MEMORY, CAPS, GPU, AGENTS). Explorations in gold, bindu, aether, and records are not canon.
- Binding languages: English and French.
- Identity constraint volunteered by the product itself: honesty about Preview vs the long-term seL4 track.

## Evidence on Hand

Real, usable:

- Product copy and structure in `website/index.html`, `website/styles.css`, `website/app.js`
- README at repo root; INSTALL, PRODUCT, FIRST-RUN, TESTER, STATUS, vision, functional and technical specs under `docs/` (EN + `docs/fr/`)
- GitHub repository and Releases
- In-app tutorial and tester protocol (not screenshots committed as marketing assets)

Must not fabricate: testimonials, named customers, benchmark numbers, press quotes, “production” screenshots, user counts, or claims that Preview is a bootable OS.

## Product Principles

1. Thesis before download — a tester must grasp why this is an OS for agents, not another chat app, before the install CTA.
2. Explain, then invite — site copy teaches what each surface does and how to extend Preview; it does not bark orders.
3. Honesty is a feature — Preview limits (host app, NVIDIA, not bootable) stay visible; they are not fine print.
4. Bounded autonomy — capabilities, audit, and deny-by-default network are the product story, not decoration.
5. Same facts in EN and FR — no language gets a softer or more complete claim.
6. Proof over theater — only ship claims the repo, releases, or a real run can back.

## Accessibility & Inclusion

Preview targets **WCAG 2.2 Level AA** for in-app chrome and primary flows (contrast, keyboard, focus, motion). The public site keeps skip-link / semantic structure and bilingual EN/FR. See [UI.md](UI.md) for navigation, status bar, theme tokens, and copy rules.
