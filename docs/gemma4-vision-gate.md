# Gemma 4 E4B vision gate (coordinator)

Coordinator gate before merge: **text infer** and **vision infer** must pass (no `ChatTemplate` error).

## Prerequisites

Weights (catalog filenames, same directory):

| File | Source |
|------|--------|
| `gemma-4-E4B-it-Q4_K_M.gguf` | [unsloth/gemma-4-E4B-it-GGUF](https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF) |
| `mmproj-gemma-4-E4B-it-F16.gguf` | same repo (`mmproj-F16.gguf` on HF → rename to catalog name) |
| `catalog-offerings.json` | copy from `share/models/` beside the GGUFs |

Suggested layout:

```text
var/models/gemma4-gate/
  gemma-4-E4B-it-Q4_K_M.gguf
  mmproj-gemma-4-E4B-it-F16.gguf
  catalog-offerings.json
```

CPU-only (matches Preview testers on 16 GB RAM):

```bash
export AOS_CPU_ONLY=1
```

## Gate 1 + 2 — direct (`aos-llama`, fastest)

Exercises `render_prompt` → `generate` and `generate_with_images` (mtmd prefill).

```bash
cargo build -p aos-model --bin aos-gate-gemma4-vision --no-default-features --release
AOS_CPU_ONLY=1 ./target/release/aos-gate-gemma4-vision --direct
```

Expect:

```text
  ✓ G1 texte seul (generate) — … généré=N …
  ✓ G2 vision (generate_with_images / mtmd) — prompt_positions=N …
GATE GEMMA4 VISION: PASS
```

Failure with `ChatTemplate` means the Jinja fallback did not run or the template still cannot render.

## Gate 1 + 2 — bus (`model.infer`, Preview path)

```bash
cargo build --release -p aos-ipc --bin aos-busd
cargo build --release -p aos-model --bin aos-modeld --no-default-features
cargo build -p aos-model --bin aos-gate-gemma4-vision --no-default-features --release

aos-busd &
AOS_CPU_ONLY=1 aos-modeld demo/gemma4-gate.yaml &

AOS_CPU_ONLY=1 ./target/release/aos-gate-gemma4-vision
```

Expect:

```text
  ✓ G1 model.infer texte seul — … généré=N …
  ✓ G2 model.infer + PNG (mtmd) — prompt_positions=N …
GATE GEMMA4 VISION: PASS
```

## Preview 0.12.0 tarball (manual)

After installing `local:gemma-4-e4b` from the catalog UI or session offerings:

1. `model.load` `local:gemma-4-e4b` — should succeed (~5.6 GiB RAM, 28 layers, mmproj `gemma4a`).
2. Text chat / `model.infer` without images — must **not** return « échec d'application du chat template » within ~1 s.
3. Paperclip / image chip: send a local PNG on the last user turn — must stream tokens (mtmd prefill), not ChatTemplate.

Environment:

```bash
export AOS_CPU_ONLY=1
```

## Download helper

```bash
export PATH="$HOME/.local/bin:$PATH"   # if using pip huggingface_hub
mkdir -p var/models/gemma4-gate
cp share/models/catalog-offerings.json var/models/gemma4-gate/
hf download unsloth/gemma-4-E4B-it-GGUF gemma-4-E4B-it-Q4_K_M.gguf --local-dir var/models/gemma4-gate
hf download unsloth/gemma-4-E4B-it-GGUF mmproj-F16.gguf --local-dir var/models/gemma4-gate
mv var/models/gemma4-gate/mmproj-F16.gguf var/models/gemma4-gate/mmproj-gemma-4-E4B-it-F16.gguf
```
