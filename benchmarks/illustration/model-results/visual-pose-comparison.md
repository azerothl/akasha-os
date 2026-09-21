# Pose reference lifecycle: actual comparison

All runs use the current pass prompts, Klein 4B seed 42, and the same saved
subject/construction from `klein4b-guided-05`. This fixture faces left and differs
from the recent right-facing automatic plans that produced three arms. The guide
is the pre-existing `klein4b-pose-experiment.png`, inspected before use. It already
shows two arms, chair perspective and an undecorated head; it is not generated
automatically by this experiment.

| Run | Guide use | Inspected final |
| --- | --- | --- |
| visual-pose-01 | Initial plus every subsequent pass | Two arms, recognizable gardener/pipe/garden; joint circles and perspective guides survive |
| visual-pose-02 | Initial only, then previous image | Two arms, no obvious knee circles; protruding furniture guides and toe-shaped footwear remain |
| visual-pose-control | No guide | Also two arms; recognizable scene but floating hand/pipe contact, toe-shaped footwear and stray construction lines |

The guide bootstrap preserves the input pose and avoids repeatedly conditioning
finishing on an unfinished sketch. However the no-guide control also has two
arms. This does NOT demonstrate that a reference fixes the earlier three-arm
failure; that would require a matched test of a failing case. None of these
finals is accepted as finished professional artwork. The first guided pass is
almost a copy of the supplied study; do not present it as newly authored pose
planning. All five stages are genuine engine calls with archived outputs.

The benchmark now optionally accepts a pose image, archived beside outputs.
`generate_pose_guided_passes` is available in aos-sd, not wired into the platform
request or automatic planner. Existing persistent image-reference behavior is
unchanged. Unit tests verify both modes: the previous pass stays first, persistent
references do not accumulate, and bootstrap-only guides never reappear in later
passes. All 25 aos-sd library tests pass. This does not establish visual quality,
automatic pose authoring, or desktop integration for this new bootstrap mode.
