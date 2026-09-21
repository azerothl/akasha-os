# Construction separation experiment

Same local models and sampling settings as `../klein4b-guided-01/review.md`.
The construction brief was manually authored in `../../skeleton-experiment.txt`.
It is a benchmark fixture, NOT evidence of automatic scene planning.
The original subject and construction text are retained in this run directory.

Inspected skeleton, volumes, contours and final PNGs. Exit code 0.

- Initial pass now omits finished clothing, face and environment, but is a
  mannequin outline rather than a pure line skeleton. The pipe is detached
  from the mouth in this early drawing.
- Volume pass preserves the pose and adds volume shading. It does not yet
  clearly distinguish a chest mass and tilted pelvis: construction quality
  remains limited.
- Contours preserve too many mechanical joint circles and guide edges while
  prematurely detailing the face and garden.
- Final is recognizable but retains shoulder/knee joint circles and a boxy
  chair. Reject as a finished illustration; coherence is not enough.

Earlier experiments: guided-02 still produced a prematurely detailed initial
pass, with an extra arm; guided-03 interpreted the chest construction instruction
as visible ribs. These failures motivated separating geometry from identity and
using solid-mass wording rather than medical anatomy wording.

Next change: contour pass must replace construction marks while preserving pose,
not preserve their outlines. All desktop and automatic planner tests remain open.
