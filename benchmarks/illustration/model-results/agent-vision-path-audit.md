# Agent illustration vision-path audit

Code inspection, 2026-09-19. This is not an end-to-end agent benchmark.

Follow-up: `provider-vision-gardener-01/review.md` records the subsequent image
transport implementation and real local comparison. The gaps below describe
the pre-fix inspection, not the current backend's transport capabilities.

## Confirmed gaps

- Both the worker and room loop checked `session_model_has_vision` before
  attaching illustration previews, then silently cleared pending references
  when it returned false. The agent still received the successful tool JSON.
- `session_model_has_vision` requires a resident model with `has_vision`.
  Registering an OpenAI-compatible provider does not establish that capability.
- `RemoteOpenAiBackend::infer_stream` serializes message text, not
  `InferRequest.images`. Merely adding a vision-capable Ollama model as a
  provider therefore does not establish visual review through Akasha.
- The checked-in, already modified local model catalog describes the Qwen
  chat model as text; no projector is configured there. This is not evidence
  about all possible models installed on the user's machine.

## Change made

Both agent loops now insert explicit attachment feedback before the next
inference. With no usable vision, the feedback says the images were not seen,
forbids inferring visual correctness from JSON, and requests an honest pending
visual-review report. With vision, it distinguishes the current preview from
the ordered original/candidate comparison. Generation progress can still be
polled normally.

This is a prompt-level guardrail, not a hard authorization gate or a reliable
artistic critic. It does not enable remote vision and does not prove the agent
will comply. It must not be reported as autonomous illustration quality.

## Verification and next evidence required

`cargo test -p aos-agent --lib illustration_ -- --nocapture`: four tests pass,
including absent-vision feedback and original/candidate attachment ordering.

The full live path still needs a genuinely image-capable inference target,
confirmed image delivery, an agent-driven generation and visual comparison,
and inspection of the resulting illustration against the original request.
Provider multimodal support requires explicit capabilities and privacy-safe
image transport, not simply bypassing the existing vision check.
