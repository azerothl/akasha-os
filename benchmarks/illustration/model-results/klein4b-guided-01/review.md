# Real local editing run: Klein 4B

Backend: stable-diffusion.cpp release master-881-17860c0, Windows CUDA.
Runner: crates/aos-sd/examples/illustration_passes.rs. Exit code 0.
768x768, Euler, 4 steps, CFG 1, seed 42, CPU weight offload and diffusion FA.
Every pass after the first received the preceding PNG through `-r`, not img2img.
Original prompts are saved beside the images. No procedural recipe or placeholder.

Models downloaded with Hugging Face CLI and verified with `hf cache verify`:

- leejet/FLUX.2-klein-4B-GGUF, revision 3b1f5a9dc3abb32238b053aeb3d823c30afdacbd,
  flux-2-klein-4b-Q8_0.gguf, SHA256 0bba6951258ec8f92d51114a8fa13e66828297bfff58a738f52729b3ef66fa28.
- unsloth/Qwen3-4B-GGUF, revision 22c9fc8a8c7700b76a1789366280a6a5a1ad1120,
  Qwen3-4B-Q4_K_M.gguf, SHA256 f6f851777709861056efcdad3af01da38b31223a3ba26e61a4f8bf3a2195813a.
- Comfy-Org/flux2-klein-4B, revision 5f526678002e43af5551dadb73ce2e8c91b43afe,
  split_files/vae/flux2-vae.safetensors,
  SHA256 868fe7b343cc8f3a19dbcfcafbc3d5f888802be3f89bd81b65b3621a066ce8f3.

Visual inspection of skeleton, volumes and contours: coherent seated elderly
person with pipe, armchair and garden, a substantial improvement over the JSON
coordinate generator. Pose, composition and identity remain similar across edits.
However the initial skeleton image is already a detailed illustration, not a
gesture skeleton. The volume pass adds guide lines over finished clothing instead
of developing simple anatomical masses. Thus the requested construction-pass
workflow FAILS despite much better subject recognizability. Do not relabel this
run as a successful traditional multi-pass demonstration.

Not tested: desktop integration, live pass publication, other prompts or animation.
Next experiment: phase-first prompts with an explicit sparse construction target,
and visual rejection before advancing from a prematurely finished initial pass.
