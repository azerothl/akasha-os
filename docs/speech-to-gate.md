# Host ASR → text for the tool gate (#433)

**Language:** English | [Français](fr/speech-to-gate.md)

If Preview has a vocal UI, **speech-to-text happens on the host** before
`evaluate_gate`. akasha-model stays text/JSON Path A–D. **No** native audio
encoder and **no** STT weights in this repository.

## Opt-in paths

1. **Paste / OS STT** — `speech.ingest_transcript` with the transcript string.
2. **Host command** — set `AOS_ASR_COMMAND` to an operator-owned transcriber
   (stdout = text). Example shape: `/usr/local/bin/my-stt --model local`.
   `speech.transcribe` then takes a **host filesystem** audio path. Missing
   command → `asr_not_configured` (fail closed). Never downloads a model.

Always-on STT is out of scope (anti-roadmap). Microphone PCM stays
`device.mic.capture` ([device-capture.md](device-capture.md)).

## Trust

Transcripts are **untrusted** evidence (same injection policy as chat).
If the operator also typed (`typed_text`), source-trust is **mixed**.
Untrusted-only still cannot authorize high-impact tools
([tool-gate-host.md](tool-gate-host.md)).

Python: `aos_gate.asr.overlay_asr_transcript` on `evaluate` / `dispatch`
when the request includes `asr_transcript`.

## Caps

| Intent | Cap |
|--------|-----|
| `speech.ingest_transcript` | `speech.ingest:*` |
| `speech.transcribe` | `speech.transcribe:*` |

Audit rows store character counts, not audio bytes.

## Module

[`community/modules/voice-to-gate/`](../community/modules/voice-to-gate/) — paste UI.
