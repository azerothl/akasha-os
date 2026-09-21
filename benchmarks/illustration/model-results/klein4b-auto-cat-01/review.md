# Automatic planning: sleeping cat

Original prompt: un chat endormi sur un coussin, vue trois-quarts, silhouette claire.
The planner was local qwen3.5:9b, temperature 0.2, seed 42, with no handcrafted
construction fixture. Raw planner request/response and parsed plan are retained.
Image backend/settings are the same as klein4b-guided-01. Exit code 0.

Inspected initial, contours and final images. Cat and cushion are recognizable,
but joint circles and internal connecting lines persist into the final. Human-
specific detail instructions introduced fabric-like folds onto the cat's body.
This is a FAIL, not a completed illustration.

A separate manual targeted edit (targeted-repair.png, log retained) asked:
"Remove the mechanical joint circles and connecting rods from the cat. Replace
the fabric-like folds on its body with smooth natural white cat fur. Keep the
exact same sleeping pose, cushion, framing and illustration style. The cat is a
living animal with ordinary natural legs, not a robot or mannequin."
Same settings and seed, with passes/4-final.png as the reference.
This removes most visible mechanical structure while preserving composition, but
small joint markers remain around the hindquarters/tail. Still not accepted.
This correction was manually authored; there is no automatic critique/repair loop.

Changes after the test: shortened contour instructions to focus on removal;
made details species-aware and prohibited clothing on animals unless requested.
Those new prompts need a fresh end-to-end image test. Technical tests pass (22).
