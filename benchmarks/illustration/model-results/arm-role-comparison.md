# Arm-role ablations

Real local Klein 4B runs. Images inspected directly, not accepted using the
experimental critic. This is diagnostic evidence, not a quality success.

- `klein4b-arm-roles-01`: seed 42, saved semantic-auto-gardener-02 frame brief,
  manually rewritten construction assigning a complete shoulder/elbow/hand
  chain to each arm. Initial and final both show three hands/arms. The final
  also retains anatomical chest lines on clothing and toe-shaped footwear.
- `klein4b-hand-on-thigh-01`: seed 42, positive explicit frame with one hand on
  the thigh and the other at the pipe; construction rewritten accordingly.
  Both inspected initial and final still add a third hand/arm. This changes
  both construction and finishing description, not a single-variable ablation.
- `klein4b-semantic-replay-seed-7`: exact saved automatic construction and frame
  from semantic-auto-gardener-02, changing only image seed 42 to 7. The initial
  and final both show three arms. This makes a seed-42-only explanation insufficient.

The failures originate before detailing and survive reference editing. Merely
expanding textual arm instructions has not fixed this case. Do not promote those
hand-authored constructions as general production fixes. The automated runner
now exposes and records image seed separately; its quote-grounding unit test
passes. A broader change to pose conditioning or rendering needs evaluation,
rather than assuming another wording adjustment guarantees correct anatomy.
