# Automatic semantic planning: rejected results

Input for both runs: `un vieux jardinier fumant la pipe dans un fauteuil
regardant son jardin`. Local Qwen3.5:9b planner, Klein 4B, seed 42, five real
reference-editing passes. No hand-authored scene description or correction.

## semantic-auto-gardener-01

The planner received the new positive-description instructions, but returned a
short frame subject without distinctive pipe parts or smoking evidence. It
invented a stool in construction, subjects and spatial relations. The inspected
final contains the stool and THREE arms (two hands on armrests plus a hand at
the pipe). Pipe and garden are readable, but smoke is absent. Rejected.

## semantic-auto-gardener-02

An experimental requested_elements list links exact user excerpts to English
visible descriptions, appended to the finishing brief. The saved plan covers
age, pipe smoking, armchair and garden-facing gaze without inventing a stool.
The new validation checks only that quotations occur in the user's text and
descriptions are nonempty; it does not prove semantic faithfulness or coverage.
Its unit test passes, including rejection of an unsupported stool quotation.

The initial image already has THREE arms: two resting hands and a pipe-holding
hand. It is also a fully detailed pencil figure, not a gesture-only construction.
The inspected final preserves all three arms; smoke, garden and armchair are
present, but anatomy is still invalid. Rejected, despite polished surfaces.
Thus semantic inventory improvement does not fix pose topology or pass fidelity.
The planner still mixes clothing and smoke into construction despite instructions
to defer them. The current production agent has not been changed to rely on this
experimental schema. Further automatic acceptance is not justified.

The next structural check should distinguish the two arms explicitly by their
complete shoulder/elbow/wrist/hand roles and contacts, rather than adding an
action hand on top of a generic seated pose. A successful isolated example would
still require multi-prompt validation and desktop end-to-end verification.
