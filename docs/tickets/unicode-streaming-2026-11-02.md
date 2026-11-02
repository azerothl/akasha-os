# Unicode streaming regression during #437 / #438 validation

Real `model.infer` requests reproduced a rendering defect with both pinned
Qwen3.5-4B Q4_K_M and Spark-X2.5-4B Q4_K_M weights: copying `🧭` returned
`���`, while copying `été 猫` succeeded. Each token was converted separately
with `String::from_utf8_lossy`; the compass character spans several tokens.

The backend now accumulates incomplete UTF-8 bytes until the next token. Plain
generation, lookup generation (including accepted drafts), and every individual
batch slot use this decoder. Only genuinely invalid bytes are replaced.
End-of-generation/max-token truncation flushes an incomplete tail; cancellation
or pause does not emit more buffered output. Decoder state belongs to a single
response and is never shared between sequences or subsequent requests.

## Observed validation

Windows, RTX 4080 SUPER 16 GiB, CUDA toolkit 13.3. Weight hashes match the pinned
assets in `model_conversation_probe.rs` and `docs/spark-x25-validation.md`.

- The same four real IPC requests after the fix returned the expected strings
  for both models, with zero replacement characters. Owned daemons were stopped.
- `unicode_stream_smoke` exercised two strings through plain, lookup and paired
  batch generation for each model on CPU and CUDA: **24/24 exact responses**.
  CUDA model buffers were observed in both GPU logs; neither CPU log contained
  CUDA model buffers. GPU placement is checked from native evidence, not merely
  the requested layer count.
- The three new decoder tests cover every byte split in `été 猫 🧭`, two
  interleaved sequences, and invalid/truncated bytes against whole-response
  lossy decoding. The complete library suite passed **23 tests** with CPU
  features and **23 tests** with CUDA/mtmd features.
- Strict Clippy passed for the library and native regression example using
  CPU features. The CUDA example also builds and runs.

Raw outputs, binary/source hashes, native log hashes and GPU buffer lines are
recorded in `evidence/2026-11-02-unicode-streaming.json`. The pre-fix binary's
exact source revision was not independently recorded; its SHA-256 identifies
that baseline without assigning it an unsupported commit provenance.

## Reproduce

```text
cargo test -p aos-llama --no-default-features --lib --locked
cargo test -p aos-llama --no-default-features --features cuda,mtmd --lib --locked
cargo build -p aos-llama --no-default-features --features cuda,mtmd --example unicode_stream_smoke --locked
target/debug/examples/unicode_stream_smoke <absolute GGUF path> 0
target/debug/examples/unicode_stream_smoke <absolute GGUF path> 99
```

Run the last two commands for each pinned model and preserve stdout JSON and
stderr logs separately. This is text-generation evidence, not a quality ranking,
vision smoke, Metal validation or installed Models UI acceptance. Native UI
validation remains open, as do #439, #444, #247, #460, #434 and #435.
