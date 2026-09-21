# Early-stage critic calibration — not reliable enough for an automatic gate

Actual PNG input: `klein4b-auto-cat-sequence-06/passes/0-skeleton.png`.
Manual inspection shows two tails, one extending left from the rump and one
upward near the back. The model is not told that count or the expected verdict.

Local benchmark `aos-sd/examples/illustration_stage_review.rs` sends image bytes,
single-frame brief and current drawing phase to an installed vision model. It
saves input, prompt, raw response and parsed review in a new directory. It does
not generate images, change a production document, or approve final art.

## Observed results

- `stage-review-cat-invalid-01`: Qwen3.5:9b rejects for alleged disconnected limbs
  but omits the duplicate tails. Its proposed correction is not validated.
- `stage-review-cat-invalid-02`: explicit per-part enumeration still produces
  tail count 1. Qwen rejects for limb connections, not the visible duplication.
- `stage-review-cat-control-01`: Qwen rejects the single-tail airborne drawing
  from sequence-04 with dubious claims that reaching paws cannot be part of a
  leap. This control is not perfect art, but does not have the duplicate-tail bug.
- `stage-review-cat-gemma-01`: installed Gemma3:12b (local API reports vision
  capability) returns ACCEPT and tail count 1 on the actual two-tail image.
  This is a false acceptance, despite valid JSON and a successful process exit.

Conclusion: neither tested local critic demonstrates dependable topology
verification. Do not connect their accept flag to automatic stage advancement or
artistic approval. An ensemble vote would also miss this shared error. Additional
calibration or a stronger visual approach is needed before enabling such a gate.

Technical follow-up: aos-sd's temporary-test-directory timestamp collisions were
mitigated with an atomic per-process suffix. All 23 tests pass after that change.
Those tests prove sequencing/path behavior, not the quality of the visual critic.
