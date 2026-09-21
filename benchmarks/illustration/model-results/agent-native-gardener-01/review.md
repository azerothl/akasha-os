# First native autonomous illustration run — failed

Real isolated bus `127.0.0.1:24719`; platform, modeld, agentd and native worker.
Model: explicitly selected local `provider:illustration-local-test:gemma3:12b`.
Session `sess-1789847197485`, agent `agent-1`. Original user gardener request,
no manually authored construction, pose guide or correction.

After 29 seconds / 13 steps, the agent reported Done and claimed that the image
was being generated. Authoritative session state had only a brief, no image_run,
no previews and no PNG. **No illustration was generated.**

The recorded cognitive state exposes a stale mandatory plan stage:
`Compose (illust.compose / puppet)`. It conflicts with the image-first protocol
and the available tool list. Repeated malformed/empty model actions also occur.
The model-based goal verifier accepted a premature completion claim.

Changes prompted by this evidence:

- A separate raster plan names generate_image, actual pass polling and inspection.
  The vector composition plan is preserved for vector-only tool configurations.
- Plan advancement checks actual tool outcomes and terminal image status rather
  than advancing on every poll or repeated set_brief.
- The worker checks live illustration state before goal.complete for image
  pipelines. No generation, running/failed generation, absent final PNG and an
  unresolved retouch all prevent successful completion. goal.fail remains usable.
  This technical guard is explicitly not an artistic approval gate.

Six illustration-focused agent tests and binary checks pass. A subsequent run
is needed to measure whether this fixes actual autonomous generation. It does
not prove that Gemma follows the tools reliably or produces good construction.

Deployment note: rebuilding the shared worker executable was refused by Windows
because another worker was using it. Inspection found live `agent-2` (PID 49904),
created outside this probe with a different user directive. It was not stopped,
modified or approved. The new guard and raster plan are source/test changes;
the shared running worker binary still predates them. A separate build location
or a later user-approved restart is required before the next live rerun.
