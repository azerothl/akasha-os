# Spark-X2.5 catalogue candidate (#437 / #438)

The llama.cpp binding is pinned to `llama-cpp-sys-2 = 0.1.159`. The mtmd image
loader call is adapted to its new initialization-options argument. CPU-only
and Windows CUDA/mtmd builds both pass the 20 aos-llama tests.

Catalogue 0.3.6 adds an **optional, experimental** chat offering:

| Field | Value |
|---|---|
| ID | `local:spark-x2.5-4b-q4_k_m` |
| Publisher | [XHToken/Spark-X2.5-4B-GGUF](https://huggingface.co/XHToken/Spark-X2.5-4B-GGUF) |
| Revision | `9826e0be84e6e6e8b9668abc91421109a1df1e2d` |
| File | `Spark-X2.5-4B-Q4_K_M.gguf` |
| Bytes | 2,600,224,352 |
| SHA-256 | `adfcfa19a4ed6a5985da8bf565fe15f8e1a7e131d79bae2d19d48d1c40109428` |
| Licence | Apache-2.0, as declared by the publisher |

The URL pins that revision, not `main`. The downloaded file's SHA-256 was
checked locally. No hardware pack default changes: users select/download the
optional model explicitly. Required quality/speed fields are zero placeholders,
and the description explicitly says unranked/unbenchmarked. They are not
measured zero-quality scores. Context budgeting conservatively accounts for
full FP16 K/V storage (147,456 bytes/token); hybrid-cache efficiency is not
assumed without measurement.

## Actual local inference

Windows, NVIDIA RTX 4080 SUPER 16 GiB, CUDA toolkit 13.3. The existing `smoke`
example applies the model's chat template, loads the GGUF and generates tokens.

| Model | CPU | CUDA |
|---|---|---|
| Spark-X2.5-4B Q4_K_M | Loaded and generated `Paris` | Loaded and generated `Paris` |
| Existing Qwen3.5-4B Q4_K_M | Loaded and generated `Paris` | Loaded and generated `Paris` |

The request asks for the capital of France in one word. These are generation
smokes, not quality, throughput, context-length or stability benchmarks.
The normal Qwen offering remains unchanged. CPU generation uses zero GPU
layers; CUDA generation requests full offload. The catalogue test verifies the
Spark URL/hash/size, optional flag and absence from automatic defaults.

Still needed for full acceptance: the installed Models UI download→load→chat
flow, a representative prompt set, and macOS Metal regression evidence. No
vision inference claim is made merely because mtmd compiles.

## Two-GPU evidence (#435, kept open)

`cargo run -p aos-llama --example multi_gpu_smoke --locked -- <model.gguf>`
records a single-GPU run followed by a two-GPU split. Capture both stdout and
stderr: nonzero model buffers on two devices must appear in the backend log,
not just a requested split in JSON. Compare outputs/errors with the existing
E9 protocol. The runner refuses fewer than two physical backend GPU devices
with exit code 77; compile-time device capacity is not used as a device count.

On this host it detected **one** CUDA device and returned the expected blocked
result. The user confirmed there are no additional machines available and
requested that #434 and #435 stay open. This helper does not substitute for
the real two-GPU or multi-machine cohort evidence.
