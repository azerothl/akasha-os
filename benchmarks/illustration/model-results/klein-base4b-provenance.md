# Experimental undistilled checkpoint

- Repository: https://huggingface.co/leejet/FLUX.2-klein-base-4B-GGUF
- Pinned revision: `d12671125306ca6b5f6db1b33ed4c80c8511a53f`
- File: `flux-2-klein-base-4b-Q8_0.gguf` (4.3 GB)
- SHA-256: `197FBC64147D4F0A0DDF4C9E667B05A3790A73ED12F92D391F0F4D286EE30FE8`
- Downloaded to the separate local cache directory `klein-base4b`; existing
  distilled model and production configuration left intact.
- Retrieved with `uvx --from huggingface-hub hf download`, after `--dry-run`.
  Public ungated repository, no login or cloud inference used.
- `hf cache verify` exited successfully and checked the one downloaded file.
  Missing remote files and extra local cache metadata were reported because this
  was a single-file download, not a complete repository checkout.

Upstream sd.cpp documents the base checkpoint with Qwen3 4B/Flux2 VAE, CFG 4,
20 steps: https://github.com/leejet/stable-diffusion.cpp/blob/master/docs/flux2.md
The publisher model card identifies an undistilled Apache-2.0 reference-editing
model: https://huggingface.co/black-forest-labs/FLUX.2-klein-base-4B

The benchmark-only profile has a passing unit test ensuring the existing
distilled default remains CFG 1 / 4 steps. Upstream settings are not a guarantee
of anatomy, action fidelity or artistic quality; inspect actual saved outputs.
