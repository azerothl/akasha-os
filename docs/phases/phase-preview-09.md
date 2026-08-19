# Phase P09 — Preview 0.9.0

**Language:** English | [Français](../fr/phases/phase-preview-09.md)

## Goal

Ship **Akasha OS Preview 0.9.0**: **mid-token device migrate** (E18) and
**extensible local media generation** (E19) — more image models (Flux2,
Ideogram4, …), a closed set of sd.cpp / Piper options, a Settings + intent
surface to pick them, and **chat media plugins** (Image studio page +
in-chat TTS options card). After E17 (0.8), device switch still cancels
the in-flight `model.infer`. 0.9 keeps the **same stream** on migrate, and
stops hard-coding `-W 512 -H 512 --steps 20` / Piper defaults.

Depends on P08 (E16 engines + E17 artefact). Not seL4 / bare metal. Not a
new P6 gate number. Not arbitrary CLI argv from agents. Not a webview.

Priorities: [evolution-roadmap.md](../evolution-roadmap.md) Horizon B
(E18, E19). Sequencing: P09.1 → P09.2 ; P09.3 ∥ P09.4 → P09.5 ∥ P09.8 →
P09.7 → P09.6.

## Why after 0.8

0.8 must ship a unified artefact, a cancel-then-reload path, and a working
sd.cpp / Piper download. Copying KV across llama.cpp backends is a separate
fail-closed change. Passing extra sd.cpp flags is also separate: 0.8 only
forwards `-m -p -o -W -H --steps`. Unknown flags must be **refused** (same
fail-closed rule as E15 widget kinds) — never `Command::arg` of free-form
strings from an agent.

## Why a closed option schema

sd.cpp and Piper expose dozens of CLI flags. Agents must not get a raw
shell. Preview 0.9 publishes a **closed JSON object** on
`media.image.generate` / `media.audio.generate` and in Settings. Keys not
in the schema are dropped/refused and audited. Per-model extras that the
engine needs (VAE, CLIP, T5, LoRA) live on the **offering**
(`extra_files` / `engine_args` in `catalog-offerings.json`). The Image
studio and TTS chat card **select among those declared ids** — they never
accept a typed filesystem path.

## Deliverables

| # | Evolution | Deliverable | Status |
|---|-----------|-------------|--------|
| P09.1 | E18 migrate | Placement Manager can move an active infer CPU ↔ GPU without aborting the stream; KV/state owned by Model Subsystem | done |
| P09.2 | E18 policy | UI pin and `auto` (load / E16 VRAM pressure) use migrate when a stream is live; 0.8 cancel+restart remains the fallback | done |
| P09.3 | E19 schema | Closed option objects on `media.*` + persisted Settings; unknown keys fail-closed; `aos-sd` maps them to allowlisted sd.cpp / Piper flags | done |
| P09.4 | E19 catalogue | Extra **optional** image packs (at least Flux2-class + Ideogram4-class) and extra Piper voices; `extra_files` for VAE/CLIP/T5/LoRA; Placement `min_vram_mib` | done |
| P09.5 | E19 surface | Models / Settings: pick default image pack + options; pick Piper voice + synthesis params; `/image` and tools honor them | done |
| P09.8 | E19 chat plugins | Host **Image studio** page (style / LoRA / VAE / …); control on a chat image switches to it; TTS ask opens an in-chat options card before generate | done |
| P09.7 | Hygiene | Finish P08.8 leftovers: split remaining oversized host modules; leftover bilingual chrome; chat role key | done |
| P09.6 | Docs / ship | FEATURES/STATUS/TESTER, version 0.9.0, site, packaging | done |

Catalogue (once shipped): [`docs/FEATURES.md`](../FEATURES.md).

### Image options (allowlist → sd.cpp)

Mapped from the schema; 0.8 values remain the defaults:

| Schema key | CLI | Default |
|------------|-----|---------|
| `width` / `height` | `-W` / `-H` | 512 |
| `steps` | `--steps` | 20 |
| `cfg_scale` | `--cfg-scale` | engine default |
| `seed` | `--seed` | random |
| `sampling_method` | `--sampling-method` | engine default |
| `negative_prompt` | `-n` | empty |
| `threads` | `-t` | engine default |
| `style` | catalogue style id (prompt prefix and/or mapped flag) | none |
| `lora` | `--lora` | none; catalogue LoRA ids only |
| `vae` | `--vae` | pack default; pick among offering `extra_files` |

Offering-owned (not agent-invented paths): `--vae`, `--clip_l`, `--clip_g`,
`--t5xxl`, `--diffusion-model`, `--lora` when the pack / catalogue extra
declares them. The studio lists those ids; it does not take a raw path.

### Piper options (allowlist)

| Schema key | CLI | Role |
|------------|-----|------|
| `length_scale` | `--length_scale` | speaking rate (higher = slower) |
| `noise_scale` | `--noise_scale` | generator variation |
| `noise_w` | `--noise_w` | phoneme-width variation |
| `sentence_silence` | `--sentence_silence` | seconds after each sentence |
| `speaker` | `--speaker` | multi-speaker voice id |

Plus extra optional Piper packs in the catalogue (beyond `en_US` / `fr_FR`).

### Chat media plugins (P09.8)

0.8 drops PNG/WAV into the transcript and fires `/image` / `/speak` with
Settings defaults. 0.9 adds two **host** surfaces (not WASM marketplace
plugins, not E13 webview):

1. **Image studio** — a first-class page (sidebar, same chrome as Models)
   for advanced sd.cpp: prompt, negative prompt, size / steps / CFG / seed /
   sampler, plus catalogue-declared **style**, **LoRA**, and **VAE**.
   Every chat message that shows an image gets a control that **switches**
   to this page with the prompt (and the PNG as preview) prefilled.
   Generate still goes through `media.image.generate` + cap `media.generate`.
2. **TTS chat card** — when the user asks to turn text into audio (`/speak`,
   or a clear TTS request in the turn), the host opens an **inline chat
   plugin** (E15-style card: voice select + Piper knobs + Generate).
   TTS does **not** auto-fire with hidden defaults on that path; the card
   is how the human picks options. An agent may still call
   `media.audio.generate` with a closed options object.

Prefill from a chat image is navigation + prompt reuse, **not** img2img as
a first-class intent.

### Cleanup / refactor leftovers (P09.7)

P08.8 landed the first host hygiene pass (egui runtime/cmd split, modeld
`media`/`providers` modules, provider presets, agent/Notes/Tasks/Audit
i18n, dead code). **Still oversized / drifted — do this in 0.9**, after
E18/E19 land and **before** tag (same placement as P08.8):

- Split `crates/aos-ui-egui/src/main.rs` (~4850 lines: Chat, Settings,
  event-apply) along existing tab/service boundaries.
- Split `crates/aos-platform/src/bin/aos-platformd.rs` (~3295 lines) along
  existing intent groups (`fs.*`, `mem.*`, `module.*`, …).
- Leftover bilingual chrome: Scenarios tab (still mostly hardcoded French);
  remaining French-only chat/status strings.
- Chat display role key is still `"vous"` (internal filter/persistence, not
  an i18n label) — move to a language-neutral `user` without breaking
  loaded sessions.

Gate: behavior-preserving (`cargo test --workspace` + p4/p5 unchanged).
Not a seL4 rewrite, not a new UI toolkit.

## Exit gates

| Gate | Criterion |
|------|-----------|
| P09.1 | Start a long GPU (or CPU) completion; switch device mid-stream; reply continues with no Stop and no truncated “cancelled” turn |
| P09.2 | `auto` under VRAM pressure migrates without the tester hitting Stop; pin gpu/cpu still overrides; failed migrate → 0.8 fallback + audit |
| P09.3 | Settings / intent can set steps and size; an unknown option key is refused (audited); generated PNG is not always 512² / 20 steps |
| P09.4 | Catalogue lists ≥1 extra image family (Flux2 or Ideogram4) and ≥1 extra Piper voice; Download + `model_id` uses it; VRAM accounted |
| P09.5 | Tester picks a non-default image pack and a Piper voice in the UI; `/image` and TTS use that choice after restart |
| P09.8 | Chat PNG shows a control that opens Image studio with prompt prefilled; studio exposes style / LoRA / VAE from the catalogue; a TTS ask opens an in-chat options card and Generate runs `media.audio.generate`; unknown keys still refused; no webview |
| P09.7 | Hygiene PR(s) land with no intentional behavior change; `main.rs` / `aos-platformd.rs` split along existing boundaries; Scenarios + chat role key follow Settings language |
| P09.6 | FEATURES/STATUS/TESTER + version 0.9.0 |
| Regression | `cargo test --workspace`; gates p4/p5 green on CUDA host |
| Packaging | Two tester artefacts (Win / Linux) + complete `latest.json` |

## Out of scope

Video generation (Wan / LTX / …), img2img / inpaint as a first-class
intent, always-on microphone / STT, raw CLI passthrough, E7 TPM, live HTTP
sibling daemon, E9 / P5.2 multi-GPU, E13 compositor, PC cohort close, macOS,
bare metal (E11–E13). Speculative decode / multi-backend token fusion.

## Next

Tag `v0.9.0` only when gates P09.1–P09.8 pass. After 0.9: remaining Horizon B
(E7 TPM, live HTTP adapter if a daemon is scheduled, E9 when a second GPU
exists).
