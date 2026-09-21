# Gesture construction and seed comparison

Same single-frame brief and construction from `klein4b-auto-cat-sequence-06`.
The first two stage instructions now request an artist's gesture sketch and
continuous natural masses, without generic human/mannequin joint vocabulary.
All later instructions and 768px/4-step settings remain unchanged.

Three seeds inspected, with all results retained (no best-only success claim):

- 42, `klein4b-gesture-cat-01`: FAIL. Two tails and an implausibly raised forelimb
  are visible from the initial sketch through final. No mechanical joint circles,
  but changing the construction vocabulary alone does not fix anatomy.
- 7, `klein4b-gesture-cat-seed-7`: recognizable single cat, one tail, front paws
  landing on the cushion. Much more coherent anatomy. Rear legs remain near the
  backrest; oversized cat/sofa relationship, stray graphite marks and unfinished
  furniture edges still weaken the result.
- 123, `klein4b-gesture-cat-seed-123`: recognizable cat descending from the backrest
  to the cushion, one tail and coherent visible limbs. No duplicate figure or
  mechanical joints. Some guide-line extensions remain, and it depicts a step/
  descent rather than unequivocally an airborne leap.

This tiny sample shows sensitivity to seed, not a measured production success
rate. Two images improve readability but neither proves the entire requested
jump-then-lie sequence. Early sketches still contain more contour detail than
the intended minimal gesture phase.

The low-level pass runner now accepts an optional reproducible seed and saves
it in seed.txt. Production generation still has a fixed seed and needs a separate
reproducible variation mechanism; no automatic selection by the unreliable critic.

All 23 aos-sd tests pass; platform/agent all-target check passes. The running
isolated desktop was not rebuilt/restarted with these new gesture instructions.
