# Keyframe-preservation experiment — FAIL

Current aos-sd source built in an isolated target directory; all 23 unit tests
passed. Executed all five real image passes, with seed 42 and the exact same
automatically authored construction as sequence-02. The relative output path
now works and all published references are absolute.

Change under test: construction/selected instant stays in the last three
prompts alongside the original request, with explicit no-duplication guidance.

Actual final PNG: two cats (one airborne, one lying on the couch). The airborne
cat retains mechanical joints and unnatural mannequin-like limb construction.
The unwanted human figures from sequence-02 disappeared, but subject count,
selected instant and anatomy still fail. This is not an accepted illustration.

Conclusion: retaining the original sequential request in the image prompt is
still ambiguous despite negative instructions. The planner needs a separate
single-keyframe rendering brief carrying identity/details but only one action.
Keep the original request separately for acceptance and sequence planning.
Anatomical construction remains an independent unresolved problem.

This tests image generation only; no desktop or full animation validation.
