# Illustration quality benchmark

This benchmark is for the still-image gate of the Illustration pipeline. It is
intentionally separate from the animation tests: a subject that is not
recognizable in one frame must not be animated or hidden behind texture.

## Procedure

### Separate the two runners

`cargo run -p aos-platform --example illustration_benchmark` exercises the
procedural recipes. Its structural review scores do **not** measure drawing
quality or demonstrate that a model followed the user's request.

`cargo run -p aos-platform --example illustration_model_benchmark -- "qwen3.5:9b" "un vieux jardinier fumant la pipe dans un fauteuil regardant son jardin"`
calls the local Ollama model and renders its authored geometry without recipes.
It saves the raw responses, specs, previews and model critiques under
`model-results/<timestamp>/`. This runner does not exercise the agent or desktop
UI. A model accepting its own image is not independent visual acceptance.

### Visual acceptance

The real image-editing runner with automatic pose planning is:

```powershell
cargo run -p aos-platform --example illustration_guided_benchmark -- MODEL_DIR NEW_OUTPUT_DIR "user prompt" qwen3.5:9b
```

An optional final `IMAGE_SEED` (unsigned 32-bit integer, default 42) varies the
image generator independently of the planner's fixed seed. It is recorded in
`image-seed.txt`. To compare image seeds without replanning, replay the saved
`passes/subject.txt` and `passes/construction.txt` through `illustration_passes`.
Report all tested seeds, not just the best candidate.

For an existing pose-image bootstrap (experimental):

```powershell
cargo run -p aos-sd --example illustration_passes -- MODEL_DIR NEW_OUTPUT_DIR "single-frame brief" CONSTRUCTION_FILE 42 POSE_PNG
```

The pose image guides only the first generated pass. Every later pass consumes
the preceding generated image, without reintroducing the unfinished guide.
The input is archived as `pose-reference.png`; its policy is recorded beside it.
This is reference-image editing, not a hard anatomical constraint or ControlNet.

For a matched **undistilled Klein base 4B** experiment in the lightweight
`illustration_passes` runner only, set `AOS_BENCH_KLEIN_BASE_WEIGHTS` to the
downloaded base GGUF. This selects CFG 4 and 20 steps while using the same
MODEL_DIR text encoder and VAE. Unset it to retain distilled CFG 1 / 4 steps.
The completed run saves `render-config.json` with the actual paths/settings.
This environment variable does not change the platform service's model default.
Do not compare a new planner output against an old one when attributing gains to
the checkpoint: replay the same saved construction, subject and seed.

The service and agent tool accept optional `pose_reference_png`: an existing
normalized `/downloads/...` PNG path in Akasha storage (not a host path), up to
2048 pixels per axis and 16 MiB. The service validates and snapshots it before
generation, archives the snapshot under the run ID, and records that path in
`image_run.pose_reference_png`. Retouches retain this provenance but edit the
selected finished image, not the pose guide. Omitting the field preserves the
existing text-only workflow. The agent must inspect the guide before choosing
it; this interface does not automatically create a correct pose or approve art.

The isolated production-service test can import a host fixture into its own
Akasha storage and verify guide archival, five publications and PNG export:

```powershell
cargo run -p aos-platform --example illustration_service_benchmark -- NEW_OUTPUT_DIR "single-frame brief" CONSTRUCTION_FILE --pose POSE_PNG
```

An optional correction string after `POSE_PNG` also exercises a targeted edit
while checking that the archived guide provenance survives. Manual corrections
are fixtures, not proof that the automatic critic can identify the same defects.

This does not prove a running desktop daemon has been rebuilt, nor verify an
autonomous agent conversation. Existing image-pass display uses the same
publication path; live UI verification remains separate.

It requires local Ollama, `AOS_SD_BIN`, the sd.cpp runtime dependencies on PATH,
and the Klein 4B/Qwen3 4B/VAE files documented in the runner. It saves the planner
request/response, plan and five passes. No model files are downloaded by this
command. The planner's `keep_alive=0` frees its model after the response to reduce
GPU contention. This benchmark still does not test the desktop UI; inspect the
saved `review.md` files for known failures before interpreting successful exit.

For every prompt in `cases.json`:

1. Set the requested brief and compose one scene.
2. Render the style sheet at 240 px.
3. Run `illust.review` and record all errors.
4. Export a 1024 px still.
5. Inspect the 240 px cell and the full-size still.

The case passes only when the subject and the requested prop/action are
immediately identifiable at 240 px. Grain, wobble and palette do not count as
evidence of recognizability.

Inspect each actual generated pass, not reconstructed stages derived from the
finished image:

- Skeleton: the requested gesture, connected limbs, plausible proportions,
  support and hand/prop contacts. For the gardener, the pose must be seated,
  with the pipe-holding arm connected at the shoulder and bent toward the mouth.
- Volumes: the same pose and proportions, oriented torso/pelvis masses and
  tapered limbs attached to the joints. Reject displaced masses and a stack of
  unrelated circles even if all named volumes are present.
- Contours: readable silhouettes, plausible near/far overlaps, no extra limbs,
  and suppression of internal construction boundaries. Furniture must support
  the body, not intersect the face or swallow the hands.
- Details: recognizable identity, face, hands, clothing and requested props;
  folds follow the body and pose. Texture does not excuse incorrect anatomy.
- Final: coherent composition and all requested spatial relationships, visible
  both at thumbnail and full size. For the gardener, verify age, pipe, armchair,
  garden and gaze direction rather than merely checking their labels in JSON.

Record failures and uncertainties explicitly. Technical tests, a successful
export, and a numeric structural score cannot override a failed visual gate.
The reviewer must see the images; do not score their descriptions instead.

`cat-jump-lie` also requires temporal verification: distinct airborne and lying
poses with a coherent landing on the sofa. A single still can verify one pose
but cannot prove the complete requested action.

### Product acceptance (separate from this runner)

The platform now exposes `illust.generate_image` with `session_id`, `holder` and
`construction` (pose/layout description) and `frame_subject` (one depicted
instant, including identity and distinguishing details). For successive actions,
the latter must omit earlier/later actions instead of asking the renderer to
ignore them. The full original request remains in `brief.subject` for acceptance
and sequence planning; a keyframe is not a completed animation. The saved image
run retains its frame subject through refinements. Call after acquiring the illustration
lock and setting the original brief. It returns immediately with `image_run`;
poll `illust.get` for `running`, `needs_review` or `failed`, immutable
`pass_previews` and the current `last_png`. `needs_review` is NOT visual approval.
An updated brief or composition invalidates publications from an older run.

`illust.generate_image` accepts an optional unsigned 32-bit `seed`. Omit it for
a new variation; the selected value is persisted in `image_run.seed`. Supplying
the same seed reproduces the noise configuration (the same models, prompts and
backend settings are also needed). Retouches inherit the source run's seed;
legacy runs without seed metadata use 42. Benchmarks explicitly set 42 unless
testing variations. Different seeds are candidates, not guaranteed improvements.

After viewing a completed raster, call `illust.refine_image` with `correction`
describing a focused visible defect. It edits the current PNG, preserves its
preview during calculation, adds the result to pass history and returns to
`needs_review`. The proposed edit is `image_run.candidate_png`; `last_png` stays
on the selected source until `illust.resolve_image` is called with the current
`run_id` and `keep_candidate` (true to use the edit, false to retain the original).
Export and further edits are blocked until that choice. Both versions remain in
history. The panel offers preview toggles and explicit selection buttons.
It does not accept an arbitrary source path. `illust.get` and
`illust.review` attach source then candidate to the next vision-capable agent
turn when a choice is pending, otherwise the selected preview. This wiring is not proof that the chosen
model correctly critiques the image, and no automatic artistic approval exists.

Host configuration for this first local backend:

- `AOS_SD_BIN`: compatible `sd-cli` executable; its CUDA runtime must be on PATH.
- `AOS_ILLUSTRATION_MODEL_DIR`: contains `flux-2-klein-4b-Q8_0.gguf`,
  `Qwen3-4B-Q4_K_M.gguf`, and `split_files/vae/flux2-vae.safetensors`.
- Missing engine/models are errors, never successful placeholders.

The production-service benchmark uses an isolated platform store:

```powershell
cargo run -p aos-platform --example illustration_service_benchmark -- NEW_OUTPUT_DIR "user prompt" CONSTRUCTION_FILE
```

It checks progressive document publication, persisted snapshots, review status
and PNG export. It does not exercise IPC transport, the agent planner, or desktop
rendering. The raster backend currently produces 768px images; vector export and
procedural animation are not supported for those images.

An optional final `"correction"` argument also tests an actual targeted edit,
preservation of the previous preview and publication of a sixth image.

Use `--auto-review` instead of a correction to let local Qwen3.5:9b inspect the
actual PNG and propose up to two edits. All critiques and their input images are
saved. A final critique is recorded after the edit limit; even an approving model
judgment never changes the service's `needs_review` status. This mode tests the
critic, not assumes its reliability: `service-auto-critique-01` demonstrates an
automatic correction regression. Do not replace human visual acceptance with its
exit code or the critic's confidence.

The comparative critic rejects a regressed candidate through the production
selection mechanism and stops without export. Its saved `selection-N.json`
records the retained source. An explicit correction fixture selects its result
only to exercise export; that technical selection is not artistic acceptance.

For desktop verification use an isolated `AOS_HOME`, platform storage configuration
and bus port. The desktop accepts `AOS_BUS_ADDR` (default `127.0.0.1:24701`). Do not
point a benchmark UI at personal sessions or use their storage as a test fixture.

Submit the prompt through Akasha. Verify that each pass becomes visible while
generation is running, preserves the subject and composition of its predecessor,
and remains selectable afterward. Confirm that stale preview completion cannot
replace the current pass and that errors remain visible without publishing a
placeholder as a successful illustration. A directory of exported PNGs alone
does not prove this behavior.

The benchmark is also the contract for the renderer rewrite. New renderer
changes should be compared against the saved outputs for these cases rather
than against unit tests alone.

### Experimental construction review (not a production gate)

```powershell
cargo run -p aos-sd --example illustration_stage_review -- INPUT_PNG "single-frame brief" skeleton NEW_OUTPUT_DIR qwen3.5:9b
```

This sends an actual saved image to a local vision model and saves all evidence.
It distinguishes expected construction marks from proposed anatomical defects.
Its exit status is only the model verdict/protocol status, not ground truth:
`model-results/stage-review-calibration.md` records missed duplicate tails and
a false acceptance by another installed model. Do not use it to approve artwork
or automatically advance production stages without further validation.
