# Raster-plan rerun: context loss, not successful generation

Private fixture bus 24729, independently compiled worker; no user agent stopped.
Session sess-1789847479222, agent-1. Same gardener request, same local Gemma model.

The worker used the new raster plan, but its persisted primary system message
was clipped at 8,000 characters in the middle of the set_brief schema. The
generate_image schema and action protocol were missing. The agent repeatedly
sent `description` instead of required `construction`, producing BadRequest.
It also wrote a brief with beats but no subject. No PNG was generated.

At step 20, goal.complete was rejected by the new live-state check: no generation
had been launched. The bounded run ended Failed at step 24 after 256 seconds,
not Done. This verifies the premature-completion fix but fails the user goal.

The next change introduces a compact system prompt for image-only agents with
no extra skills/documents/instincts. It retains original goal, action envelope,
image semantics and actual parameter schemas; unrelated default module tools
and OS background are omitted. Schema descriptions are compacted, not parameter
requirements. Other agent types retain the existing prompt compiler.

The new unit test verifies required construction/frame_subject and candidate
selection fields remain present within the 8,000-character hard trim limit.
Live rerun: agent-native-gardener-03. This limited prompt specialization does
not yet cover the UI's mixed raster/vector tool configuration.
