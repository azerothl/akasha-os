# Native tool decoding corpus

`fixtures/models/native-tools-v1.json` was frozen before the first inference.
SHA-256: `21680303737ffae7a82c79a1f09065f6fc9cc84ecb272dc72cef4c59040449ba`.
Its four authored cases cover no-argument calls, a Unicode workspace path,
numeric/null-looking strings, and typed booleans/integers/arrays/objects.
The first case reuses the original conversation corpus's tool request.
`probe.echo` is a declared fixture tool; no tool executes in this corpus.

Both pinned Q4 GGUFs were hashed before each profile. Each model receives two
concurrent distinct requests, followed by a second complete chronological pass,
at temperature 0, seed 42 and a 128-token response limit.

| Profile | Qwen matching calls | Spark matching calls | Identical replays | Native two-job batches |
|---|---:|---:|---:|---:|
| CPU | 6/8 | 0/8 | 8/8 | 8 |
| CUDA | 8/8 | 0/8 | 8/8 | 8 |

All 32 requests completed and owned model daemons stopped. CUDA buffer evidence
was observed for both GPU models; CPU logs contained no CUDA model buffers.
The two Qwen CPU failures change the requested filename to `prémière.txt`.
They remain failures. Spark emits its own `arg_key`/`arg_value` dialect; its
calls are unsupported by this implementation and are rejected. These figures
describe a protocol regression, not a model ranking or general quality score.

The conversation probe now grades tool requests using the same schema-aware
parser as worker/room execution. The captured native responses are also replayed
against the current parser in a CPU-only regression test, including all rejected
Spark responses. Historical initial conversation scores are kept unchanged.

Raw responses and expected values, native log hashes, model/probe hashes and
replay comparisons are retained in:

- `evidence/2026-11-02-native-tools-cpu.json`
- `evidence/2026-11-02-native-tools-cuda.json`

Run `model_conversation_probe` as documented in
`model-conversations-2026-11-02.md`, supplying this four-case fixture. The real
permission-gated project workflow is documented separately in
`native-tool-contracts-2026-11-02.md`.

Installed UI, representative task quality, Metal inference and the physical
machine/two-GPU cohort remain unvalidated here. #439, #444, #247, #460, #434 and
#435 remain open, and Spark remains optional and unranked.
