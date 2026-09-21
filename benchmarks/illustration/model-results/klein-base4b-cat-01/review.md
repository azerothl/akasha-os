# Undistilled base 4B: matched saved-plan test

Replayed the exact construction and subject from
`klein4b-semantic-auto-cat-02/passes`, image seed 42, same 768px size, Qwen3 4B
text encoder and Flux2 VAE. Changed diffusion checkpoint to base Q8_0 and its
documented sd.cpp profile to 20 Euler steps / CFG 4. No reference image supplied.
`render-config.json` records the actual configuration. Provenance/checksum are
in `../klein-base4b-provenance.md`.

Five actual engine calls completed. File timestamps place first prompt at
21:28:50 and final PNG at 21:30:56, about 126 seconds (not a dedicated performance
measurement). Initial, contour and final images were inspected directly.

The initial feline silhouette is coherent and less mannequin-like, with one
tail. But the front paws approach the armrest rather than the requested seat,
and the movement still reads ambiguously as leaving the sofa. The first pass
already includes face and anatomy outlines rather than a sparse gesture.

The final is not a quality improvement: noisy repetitive fur, heavy dark sofa
shading, uneven linework and remaining sketch lines produce a less controlled
finish. It still does not establish correct landing contact. Rejected; keep the
production distilled model unchanged. This single seed/profile/scene does not
prove that every base-model configuration is worse, nor establish a reliable
local alternative. Existing input-plan contradictions remain a confounder, but
were deliberately held constant for this comparison.
