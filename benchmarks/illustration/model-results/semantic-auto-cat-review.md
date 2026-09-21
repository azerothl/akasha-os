# Automatic cat sequence planning — current pipeline

Both runs use the original French request `un chat saute sur un canapé et se
couche`, local Qwen3.5:9b planning and Klein 4B seed 42. No manual pose guide or
correction. Five actual passes per run, inspected directly rather than accepted
through the unreliable experimental critic.

## semantic-auto-cat-01

The requested-elements inventory covered both successive actions, and all
visual descriptions were concatenated into the single-frame brief. The planner
described a hybrid mid-landing/lying-down posture. Final image: recognizable cat
and sofa, but two tails, excessive relative size and an unclear action. Rejected.

## Temporal inventory separation

The experimental inventory now requires `in_selected_frame` per element. Future
and past actions stay in the saved plan but are not appended to the image brief.
Construction, contacts and frame_subject are instructed to describe only the
selected instant. A unit test verifies that later-action evidence is not fed to
the renderer; the quote-grounding test still passes. These tests do not prove
that the planner assigns flags correctly or that resulting poses are valid.

## semantic-auto-cat-02

The plan assigns jumping to the current frame and lying down to a later frame;
the renderer brief contains only the current evidence. This fixes the inventory
handoff contradiction. However its ratios remain implausible/ambiguous (cat
height compared to cushion height; nose-to-tail length equals seat width), and
spatial statements conflict about above/below seat placement. The inspected
initial image is already a detailed feline outline above the backrest rather
than a gesture sketch approaching the seat. This is not an automatic pose-quality
success or a complete depiction of the original action sequence.

The inspected final has one tail and a recognizable cat, but the front paws
land on the backrest instead of the planned seat, the animal remains oversized,
and large unexplained rectangular background/guide edges survive. Rejected as
finished artwork. Fixing temporal evidence did not solve geometry, action contact
or finishing. This inventory schema remains in the experimental runner, not an
automatic production acceptance gate.
