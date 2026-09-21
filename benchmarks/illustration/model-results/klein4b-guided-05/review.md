# Pose-to-illustration experiment, revision 5

Same model files and settings as guided-01; same manually authored construction
fixture as guided-04. The fixture is not an automatic scene planner.
Five sequential local model calls completed with exit code 0. Each edit consumes
the preceding output. Prompts and both briefs are saved alongside the PNGs.

Inspected contours and final (the initial two prompts are unchanged from run 04).
Replacing construction outlines rather than preserving them removes the shoulder
and knee joint circles from the person. Clothing follows a recognizable seated
body, the bent arm holds the pipe, and the garden is identifiable.

Still rejected as fully finished: the armchair retains box construction lines,
edges protrude beyond the furniture and its base reads as an unfinished solid box.
The initial pass remains a mannequin outline rather than a pure stick skeleton.
Volumes and contours still need stronger stage separation. Face and hand quality
are improved over coordinate-generated shapes but have not been user-approved.

No desktop publication, automatic brief planning, multi-prompt reliability or
animation validation has been performed. Unit tests passed (22), covering only
technical contracts including exact predecessor-reference and publication order.
