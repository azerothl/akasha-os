# Provider image transport: real local comparison

2026-09-19. Actual `RemoteOpenAiBackend::infer_stream` call to installed local
Ollama `gemma3:12b`, using its OpenAI-compatible endpoint. `/api/show` confirmed
`completion` and `vision` capabilities before the run. No WAN inference.

Inputs: the original final image from `service-pose-refine-01` and that run's
exported retouch, in that order. The request asked for visible differences,
without describing the expected corrections. `response.txt` is the actual
streamed answer, not an edited assessment. The first probe version recorded
`model_id: null` in request-metadata; the backend was explicitly constructed
with `gemma3:12b`. The runner now records that model in future metadata.

## Result

The backend completed its real streaming request with both image attachments.
The model identifies footwear changing from open footwear to closed shoes.
However, it describes smoke as becoming more diffuse: the original has **no
smoke**, while the revision adds a clear plume. Its claims about hand creases
are not convincing evidence of an improvement. It also understates removal of
the long construction strokes protruding from the chair.

Conclusion: usable multimodal transport, **unreliable visual comparison**.
This is not approval of the image, a model-quality benchmark across subjects,
or proof that the autonomous agent-to-UI workflow works end to end.

## Implementation and verification

- Images are attached only to the last user message, preserving source/candidate
  order. Unit tests decode their transmitted base64 and compare exact bytes.
- Missing, invalid, oversized or unsupported image files produce an error,
  never a silent text-only fallback. No HTTP redirects are followed.
- Modeld denies provider image requests if referenced data is secret or its
  classification fails. Provider errors are returned through the inference stream.
- Local Ollama provider registration queries explicit `vision` capability.
  Agent vision checks accept an explicitly selected confirmed provider; they do
  not choose a remote provider as a fallback or infer default-model vision from
  an unrelated loaded model. Other providers remain unconfirmed.
- `cargo test -p aos-model --no-default-features --lib`: 41 passed.
  Default-feature tests failed because of an existing invalid CUDA `argsort.obj`
  (LNK1136), not a Rust test failure. No build artifacts were deleted.

Protocol reference: https://github.com/ollama/ollama/blob/main/docs/api/openai-compatibility.mdx

Remaining: live model registration/capability discovery and agent IPC test,
including actual UI pass progression, plus a substantially more reliable visual
critic. These changes have not restarted the running daemons.
