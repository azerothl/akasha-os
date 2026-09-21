# Production service: targeted correction

Real generation and correction via illustration_image::{start,refine}, using an
isolated platform store. Same original French brief and automatically produced
construction plan as service-gardener-02. Local Klein 4B settings unchanged.

Five persisted pass previews appeared at 8.3, 15.3, 22.8, 30.1 and 37.4 seconds.
The refinement kept the previous last_png while running, incremented revision,
preserved history and published a sixth image at 44.9 seconds. Final status is
needs_review, review accepted=false, and real PNG export succeeded. Exit code 0.
Publication snapshots and final export are retained.

The correction was manually authored in the benchmark invocation:
"Redraw the trousers as ordinary plain fabric covering natural human legs. Remove
every circular joint marker and internal construction line from both knees, shins
and ankles. Replace the oversized outlined smoke cloud with a small delicate
translucent wisp. Keep the seated pose, pipe, garden and illustration style unchanged."

Visual inspection of final.png: the conspicuous knee/shin construction guides
are removed and the smoke is now a light wisp. Pose, composition and setting are
preserved. This is an improvement, not proof of automatic critique or universal
illustrator-level output. Pipe shape and other fine details still merit review.

Vision wiring now captures current last_png from illust.get/review, rather than
requiring vector sheet/export. Unit test covers nested/direct documents and
rejects history-only fallback. Persisted run tests cover preview retention on
edit, stale publication rejection and terminal state protection. Cargo check
passed for platform, agent and UI. No desktop or real-agent vision turn tested.
