# Separate frame subject — improved, not accepted

Current guided runner compiled through aos-sd's lightweight example target.
Original request: « un chat saute sur un canapé et se couche ».
Qwen3.5:9b automatically returned frame_subject: "A cat leaping onto a sofa".
No human-authored frame brief. Five actual Klein 4B passes, seed 42, 768 square.

Inspection of skeleton and final PNGs:

- One cat and one recognizable sofa. No extra resting instance or human figures.
- The final cat is recognizably feline, with natural fur instead of clothing.
- Most construction marks disappear in the final; rear-leg joint contours remain
  somewhat diagrammatic. The first pass is still over-detailed for a gesture sketch.
- Relative scale is implausible: the cat is very large compared with the sofa.
- The airborne path reads as passing over the couch, not a clear approach to its
  seat. A still cannot establish a successful landing or the later lying action.

This is a substantial improvement over sequence-02/03, not an illustrator-quality
acceptance or proof of a complete animated request. The planner changed both the
construction and the frame brief; this is not a controlled one-variable ablation.
Next checks: explicit relative dimensions and landing target in the spatial plan,
then a second consistent keyframe. Desktop publication remains unverified.
