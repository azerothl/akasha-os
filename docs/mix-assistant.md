# Mix assistant host (Path A / #432)

**Language:** English | [Français](fr/mix-assistant.md)

Preview host for the akasha-model mix contracts (descriptors, preset
catalogue, confirmable `mix_proposal` batches). **Path A only** — no MASK
scorer, no audio encoder, no copyrighted audio in git.

## Flow

```text
Host DAW / analyser  →  numeric descriptors JSON  (no WAV in guest)
        │
        ▼
mix.ingest_state / mix.load_demo
        │
        ▼
mix.propose (preset + confidence vs min_confidence)
        │  ready | abstain
        ▼
Human confirms  →  mix.apply  (cap mix.apply:*)
        │
        ▼
In-memory session settings + audit row
        │
        ▼
mix.undo = inverse of the last batch (one action)
```

A DAW that moves faders stays **outside** the sandbox. This host never
executes DSP.

## Intents / `host_call`

| Name | Cap | Notes |
|------|-----|--------|
| `mix.catalog` | `mix.read:*` | Catalogue v1 (`vocal_forward`, `balanced`, `bass_heavy`) |
| `mix.demo_state` | `mix.read:*` | Synthetic descriptors (format check only) |
| `mix.load_demo` | `mix.read:*` | Ingest demo descriptors into a session |
| `mix.ingest_state` | `mix.read:*` | Validate host JSON (`schema_version: 1`) |
| `mix.propose` | `mix.read:*` | Requires explicit `min_confidence`; abstain if below |
| `mix.apply` | `mix.apply:*` | Fail-closed without `confirmation_given` |
| `mix.undo` | `mix.apply:*` | Inverse of last applied batch |

Audit envelopes hash the ready batch (`mix.apply_batch` tool name, same
confirm/undo shape as akasha-model T7).

## Module

Community reference (MIT, not in the signed zip catalogue by default):

[`community/modules/song-maker/`](../community/modules/song-maker/)

Install via the usual script Path A ([write-a-module.md](write-a-module.md));
cap review must grant `mix.read:*` and `mix.apply:*`.

## Non-goals

- Mix quality / MASK checkpoint (akasha-model T8)
- Computer-use / webview
- Shipping WAV, MP3, or third-party stems
