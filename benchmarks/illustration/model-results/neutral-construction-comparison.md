# Construction instructions leaking into finishing

Two real local five-pass runs, same model, seed 42, original French gardener
brief, and hand-authored `../../construction-gesture-neutral.txt` fixture.
This isolates the prompt handoff; it is not an autonomous planner benchmark.

## 01: construction prose repeated in all stages

Initial output is simpler than the previous gardener experiment, but already
contains anatomy outlines, fingers and toes. It is not an axis-only gesture.
The final output retains a blank face and empty environment, despite the user
asking for an elderly gardener looking at his garden. The construction fixture's
temporary omissions remain present in late prompts. Unacceptable final result.

## 02: construction prose only in first two stages

Later stages receive the single-frame subject and previous image, with explicit
instructions that construction-stage omissions are temporary. A face, elderly
identity, garden and visible smoke now appear. However, the model adds an
unrequested dog and tattoo-like forearm texture, does not produce a clear
armchair, and renders a questionable pipe shape. Style is more polished but
inconsistent with the earlier example (this fixture did not specify a style).
This result is also NOT accepted as faithful to the user request.

The source change removes a contradictory instruction handoff, not the need
for reliable semantic constraints and review. Anatomical/composition facts that
must survive should be included in the single-frame subject, not only in the
temporary construction instructions. Both runs preserve every actual pass.
The 23 aos-sd library tests pass, including reference chaining and the updated
late-prompt separation assertion. No desktop service restart or UI acceptance
was performed for this experiment.

## 03: generic prohibitions (discarded)

Same inputs and seed as 02, adding generic instructions against new pets,
tattoos and accessories, and to complete functional object parts. A dog and
tattoo-like forearm patterns still appear; the seat remains a chair without
clear armrests. The dog is already visible in the contours pass. This prompt
variant was removed from production rather than claimed as a fix.

## Explicit frame 01: positive semantic description

`klein4b-explicit-frame-01` uses the same construction, seed and engine, with
the hand-authored English `../../gardener-explicit-frame.txt`. It expands the
armchair into its functional parts, describes the pipe's stem and upright bowl,
and specifies smoke, plain clothes and a traditional ink/watercolor style.
This changes language, specificity and style together; it does not isolate
translation as the cause of improvement.

The inspected contours and final contain no added animal. The final has a
recognizable pipe, visible smoke, a near armrest and the elderly gardener facing
a flower garden. Shoes remain toe-like; an extended horizontal guide and minor
stray lines survive. The first construction pass is unchanged and still too
detailed. This is better semantic fidelity on one manually planned case, not
an autonomous system acceptance or a measured reliability improvement.

The experimental automatic planner now asks for positive visual definitions of
distinctive object parts and visible action evidence, in English. That planner
change still needs an actual automated benchmark before production promotion.
