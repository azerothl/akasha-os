# Comparative critic: regression detected

Same automatic construction input and image service as service-auto-critique-01.
The revised critic describes concrete marks and desired replacements, and after
each edit receives TWO images: source first, candidate second. Its raw responses,
parsed judgments and exact input PNGs are retained.

Critique 0 correctly identifies internal guide lines and joint symbols on the
legs. However its edit instruction says to reveal skin or fabric rather than
unambiguously preserving the existing trousers. The candidate removes guides
but turns the trousers pink. Manual inspection of critique-1-input.png confirms
this color/material regression.

Critique 1 detects the regression by comparison with the source. The benchmark
stops with exit code 1 BEFORE export and before requesting further edits on that
candidate. Six previews had been published; no final.png is exported by the
benchmark. This is successful regression detection, NOT successful illustration.

The comparison guard currently belongs to the benchmark. The production service
still exposes new candidates through last_png with needs_review; it does not yet
automatically reject/restore based on this comparison. Source PNGs remain in pass
history. Integrating candidate selection and preserving explicit source linkage
are still needed before claiming a safe autonomous repair loop.

A separate build of current UI/platform/bus binaries was started to enable real
desktop verification. It was still compiling at the time of this report; no UI
verification result is implied.
