# Community — Akasha OS Preview 0.15.1

**Language:** English | [Français](fr/community.md)

> Date: 01/09/2026 · Preview **0.15.1**

The hangout is **GitHub Discussions** on
[azerothl/akasha-os](https://github.com/azerothl/akasha-os/discussions).
There is no Discord, Matrix, or Slack for this project yet — those would
be empty rooms. Messaging channels are also **out of the OS core** (see
[evolution-roadmap.md](evolution-roadmap.md)); this page is for humans.

Public site: [Community](https://azerothl.github.io/akasha-os/community.html).

License split (kernel AGPL + CLA vs guest extensions Apache/MIT, no
commercial grant): [ADR 0006](../adr/0006-license-split.md). A Discussion is
never a CLA event. Do not PR a skill into `crates/`.

## Cohort gate

**3 Windows + 1 Linux + 1 macOS Apple Silicon** testers complete the
[15-minute path](TESTER.md#short-path-15-minutes) without a Rust toolchain.
Each leaves a usable `var/feedback/fb-*.json` (and preferably a GitHub
issue). The long TESTER protocol stays the team checklist; it is not
required of every tester.

macOS builds are unsigned (Gatekeeper). Intel Mac is not supported.

## Suggested Discussion categories

Create these in the GitHub repo UI if they are missing (Settings →
General → Features → Discussions):

| Category | Format | Description (paste into GitHub) |
|----------|--------|----------------------------------|
| **Cohort** | Open-ended | Preview tester check-ins and the cohort call. Walk the 15-minute path (Windows, Linux, or Mac Apple Silicon), then report here. Bugs go through in-app Feedback. EN or FR. |
| **Q&A** | Question and answer | Install, first run, “does this count” |
| **Show and tell** | Open-ended | Skills, modules, screenshots — not a PR |
| **Ideas** | Open-ended | Product ideas that are not bugs |
| **Research** | Open-ended | Caps, seL4, GPU placement, semantic IPC. Not first-run help — testers start in Cohort or Q&A. EN or FR. |

Built-in Q&A / Ideas / Show and tell are enough until Cohort and Research
exist.

## Maintainer checklist (once)

1. Confirm Discussions are enabled (they are on this repo).
2. Add categories **Cohort** and **Research** if missing.
3. Post the call below in **Cohort** (EN, then a FR reply or a second
   post). Pin it.
4. Confirm labels exist: `bug`, `cohort`, `area:modules`, `area:skills`,
   `good first issue`.

## Public call (paste into a Cohort Discussion)

**Title:** Preview cohort 0.15.1 — looking for 3 Windows + 1 Linux + 1 Mac

```
Akasha OS Preview 0.15.1 is an installable host app (Windows / Linux x64,
macOS Apple Silicon) — not a bootable OS.

The cohort gate is: 3 Windows + 1 Linux + 1 macOS Apple Silicon testers
complete the 15-minute path without cargo or a clone.

Short path: install from Releases → tutorial → one offline chat → one
note → Send feedback from the UI.
https://azerothl.github.io/akasha-os/community.html
https://github.com/azerothl/akasha-os/blob/main/docs/TESTER.md

macOS is unsigned; run install.sh and expect Gatekeeper. Intel Mac is
out. NVIDIA is recommended on Win/Linux; CPU-only is slower but counts.

Reply here with OS + GPU/CPU and whether chat and notes worked. File
bugs from the Feedback tab so var/feedback is attached.

There is no Discord. This thread is the hangout.
```

French body for a second post or a reply:

```
La Preview 0.15.1 d’Akasha OS est une appli hôte installable (Windows /
Linux x64, macOS Apple Silicon) — pas un OS bootable.

Gate cohorte : 3 testeurs Windows + 1 Linux + 1 macOS Apple Silicon
suivent le chemin de 15 minutes sans cargo ni clone.

Chemin court : installer depuis les Releases → tutoriel → un chat hors
ligne → une note → envoyer un retour depuis l’UI.
https://azerothl.github.io/akasha-os/community.html?lang=fr
https://github.com/azerothl/akasha-os/blob/main/docs/fr/TESTER.md

macOS n’est pas signé ; lancer install.sh, Gatekeeper avertira. Pas
d’Intel Mac. NVIDIA recommandé sous Win/Linux ; le CPU-only est plus
lent mais compte.

Répondre ici avec OS + GPU/CPU et si chat et notes ont marché. Les bugs
passent par l’onglet Feedback pour attacher var/feedback.

Pas de Discord. Ce fil est le lieu de rencontre.
```

## What testers should do

1. [Install](INSTALL.md) without `cargo`.
2. [Short path](TESTER.md#short-path-15-minutes).
3. Feedback from the UI.
4. Optional Discussion check-in.

Code of conduct: [CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md).
Security: [SECURITY.md](../SECURITY.md).
How to contribute: [CONTRIBUTING.md](../CONTRIBUTING.md).
