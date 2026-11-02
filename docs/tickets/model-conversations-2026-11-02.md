# Bounded conversation corpus for #437 / #438

The twelve authored cases in `fixtures/models/conversation-437-438-v1.json`
were frozen before the first evaluation. SHA-256:
`9690679a7f4857af5da198d2b1743a2254e22eb6968766d81ef2cb3f97936e1a`.
Prompts and expected answers were not adjusted after observing results.

The corpus covers arithmetic, strict JSON, Unicode copying, Rust expressions,
small supplied project-policy snippets, two multi-turn conversations, one system
instruction and one tool-format request. It is a synthetic contract check, not
a representative quality, security, memory-history or performance benchmark.

## Initial measured results

Source includes Unicode fix `8a834dd`. Both GGUF assets were hashed and checked against their
pinned values before every run. The real `aos-modeld` process serves requests
over a private local broker with an isolated home. No tool is executed.

Each profile sends two distinct requests concurrently, repeats the complete
corpus in a second chronological pass, and uses temperature 0, seed 42 and a
128-token response limit. Native logs show **24 two-job batches per profile**.

| Model | CPU contracts | CUDA contracts | Identical replays per profile |
|---|---:|---:|---:|
| Qwen3.5-4B Q4_K_M | 18/24 | 18/24 | 12/12 |
| Spark-X2.5-4B Q4_K_M | 10/24 | 10/24 | 12/12 |

All **96 requests completed**. Both models loaded on the requested CPU/CUDA
placement, and all owned daemons stopped. CUDA model-buffer allocations for
both models are recorded; CPU logs contain no CUDA model-buffer allocations.
Native diagnostic logs can contain raw non-UTF-8 tokenizer bytes: their hashes
cover the original bytes; tolerant decoding is limited to diagnostic searches.
Model responses are preserved as the typed UTF-8 strings received over IPC.

The strict grader rejects extra prose, fenced JSON when only an object was
requested, wrong values and unsupported tool-call forms. Closed reasoning
blocks are stripped using the production agent parser; unfinished reasoning is
not accepted as an answer. The tool grader also uses the production action
parser and requires exactly one expected action with exactly the expected args.
Two grader regression tests and strict CPU Clippy passed.

Failures include arithmetic and JSON shape for Qwen; Spark additionally fails
some code/policy answers and emits an unsupported tool form. Six cases per
model were selected **after seeing these failures** for an unbatched CPU
diagnostic control. The same arithmetic/code-policy errors occur without paired
batching. This control is not a held-out evaluation or a proof that all batching
behavior is correct. Tool failures also exposed missing declarations in native
prompt rendering: the C matcher accepts messages only, and the current Jinja
fallback receives tools only when that matcher fails to recognize the template.

A local renderer experiment confirmed that passing declarations through Jinja
changes Qwen from refusing the tool request to producing
`<tool_call><function=notes.list>...</function></tool_call>`. The production
action parser does not support this dialect, so that renderer experiment is
not included in the implementation. A follow-up must deliver rendering and
schema-aware native argument parsing together, with end-to-end agent checks;
changing rendering alone would leave tool handling incomplete. Spark's response
remained unsupported. Neither result is counted as a successful tool contract.

Raw responses, expected values, per-case grades, replay comparisons, placement
lines and source/binary hashes are in:

- `evidence/2026-11-02-conversation-cpu.json`
- `evidence/2026-11-02-conversation-cuda.json`
- `evidence/2026-11-02-conversation-unbatched-control.json`

## Reproduce

```text
cargo build -p aos-model --bin aos-modeld --locked
cargo build -p aos-platform --no-default-features --example model_conversation_probe --locked
cargo test -p aos-platform --no-default-features --example model_conversation_probe --locked
target/debug/examples/model_conversation_probe <absolute modeld> <absolute Qwen GGUF> <absolute Spark GGUF> <absolute corpus JSON> cpu
target/debug/examples/model_conversation_probe <absolute modeld> <absolute Qwen GGUF> <absolute Spark GGUF> <absolute corpus JSON> cuda
```

Capture stdout JSON and stderr separately. Run profiles sequentially on a shared
GPU host. The helper records contract failures without turning them into a
transport failure; inspect both kinds of result. It does not change installed
Preview data or catalogue defaults.

Spark remains optional, experimental and unranked. These results do not complete
the representative prompt, vision, Metal or installed Models UI acceptance.
Native UI validation and #439, #444, #247, #460, #434 and #435 remain open.
