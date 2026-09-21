# Production background-service verification

Runner: illustration_service_benchmark. Isolated PlatformSubsystem store, real
local sd.cpp/Klein models. Saved original French brief; construction came from
the earlier automatic Qwen planner output, not a new manual pose fixture.

Observed persisted publications at 7.8, 15.3, 22.8, 29.9 and 37.1 seconds.
Snapshots publication-1.json through publication-5.json prove five distinct
immutable PNG paths and progressive last_png updates while the job was running.
Terminal state: needs_review. Review returns accepted=false. PNG export succeeded
and its bytes were copied through the actual storage read API to final.png.
Runner exited 0. Runtime stores (including fresh secrets) are ignored by Git.

Visual inspection of final.png: recognizable seated older person with pipe and
garden-like surroundings. Knees and trouser legs retain conspicuous construction
guides; smoke is an oversized cartoon outline. Not accepted as illustrator-quality.

Scope limits: production service, storage and export exercised; IPC transport,
agent-driven tool selection, desktop display and restart recovery NOT verified.
The running desktop application has not been restarted or configured by this test.

Additional checks this turn: platform/agent/UI cargo check passed; 16 existing
illustration agent tests passed; new image-tool routing test passed; new persisted
run test passed (stale/terminal publication rejection and no procedural enrichment).
These technical checks do not prove artistic quality.
