# Managed project-context worker acceptance (#247)

**20 October 2026.** PR [#458](https://github.com/azerothl/akasha-os/pull/458)
was merged as `11a5fd940dad7e01f27b6d54dc4990a445388a69`, after 18
successful checks (manual CUDA skipped). This follow-up extends the
[real-model proof task](dev-assistant-real-worker.md) through production
`aos-agentd`, instead of starting the worker directly.

## Defects found by the managed lifecycle probe

The first scripted run reached `Done` but failed acceptance:

- The always-on module kit implicitly added `module.compile`, `module.install`
  and `module.uninstall` capabilities to the explicitly selected read-only
  project-context tools. Selecting native workspace-context tools now excludes
  implicit module mutations. Explicitly selecting a module tool still works;
  ordinary non-context kits keep their existing defaults. An ungranted context
  kit also no longer receives an unrelated default notes grant from agentd.
- Agentd replaced records solely by step number. Both `task.assess` and
  `memory.recall` use step zero, including repeated calls after steering.
  Live traces now identify redelivery by `(step, action, timestamp)`. Distinct
  startup records survive; an exact redelivery updates the existing record.

These changes do not grant project access from tool selection. The acceptance
task explicitly supplies only `fs.read:/host/demo/**`; both saved and live task
capabilities must remain exactly that list.

## Exercised lifecycle

Both existing examples now accept an optional agentd executable beside the
worker. A shared acceptance client:

1. Starts the actual daemon against a fresh broker and isolated temporary home.
2. Submits the task using `agent.create`, which allocates its ID, persists the
   spec/registry and launches the actual worker.
3. Subscribes to live output and polls `agent.state` until a terminal state and
   no worker PID: agentd's child waiter has reaped the worker.
4. Reads `agent.trace` and verifies the exact project grant and registry entry.
5. Stops agentd **after** the worker has finished, then starts a new daemon on
   the same home and broker.
6. Uses `agent.state`, `agent.trace` and `agent.list` to verify one restored
   terminal task, unchanged specification, permissions, profile, result and
   trace. Both owned daemon processes are stopped afterward.

Trace comparison requires exact text, ordering, structure, integers and tool
arguments. Only each step's `tok_s` and `ttft_ms` telemetry allows a rounding
difference of `8 * f64::EPSILON * max(abs(a), abs(b), 1)`. JSON checkpoint decoding
can move these floating values by a few representable values. A real-model run
first failed strict equality; that failed report remains in the evidence.
The comparator has regression checks against lost steps, changed source
results, significant timing changes and adjacent large integers.

`agent-live-events.json` is the subscription as received. `events.json` uses
steps and the terminal state from the authoritative daemon API, with other
events from the live subscription. A subscriber is not promised startup replay.
The two API traces are retained as `trace-before-restart.json` and
`trace-after-restart.json`. The report does not invent a worker OS exit code:
agentd exposes the terminal state and reaped PID, so `worker_exit` is null.

## Reproduce

Build `aos-agentd` and `aos-agent-worker` into the same target directory, plus
the platform examples without default features. The lightweight scripted task
uses no model weights and explicitly disables the model gate for its fixture:

```text
workspace_agent_context_probe ABSOLUTE_WORKER_EXE ABSOLUTE_AGENTD_EXE
```

The opt-in Qwen task keeps the model, Python/gate dependencies, pinned hash and
scoped three-file proof contract described in the previous document:

```text
workspace_real_model_worker_probe ABSOLUTE_WORKER_EXE ABSOLUTE_MODELD_EXE ABSOLUTE_GGUF --require-gate --agentd ABSOLUTE_AGENTD_EXE
```

All subprocesses use the generated directory as their working directory. No
user repository is selected. The real task also requires unchanged file hashes
and the exact original file count, a withheld synthetic credential file, a
correct previously unprompted random proof code, two gated context calls, zero
legacy steps and a verified signed audit. It downloads nothing.

The workflow adds the scripted managed lifecycle alongside the existing direct
worker scenario. It does not add real weights or Python model-gate downloads.

## Observed local verification

- **727 Rust tests passed:** 384 agent library, 22 agent binary, 319 platform
  library, and two example comparator tests. Strict Clippy passed for both
  crates and all targets without default features.
- The direct scripted scenario still passes: six planner turns, eight trace
  records and four scan pages. The managed scripted scenario also passes, with
  those eight records retained through an actual daemon restart.
- The real Qwen3.5-4B Q4_K_M managed scenario passed: five records including both
  step-zero startup events, exact fresh proof from `première.txt`, two gated
  context calls, zero legacy calls, unchanged sources, withheld synthetic
  credential, verified audit, and stopped owned daemons. The restored task had
  the same exact grant, result, model, tools and token count.
- The final before/after traces differ only at step index 3's `ttft_ms`:
  `37.396499999999996` versus `37.3965`. All transcript content is unchanged.
- This local backend observed one RTX 4080 SUPER and offloaded 32/33 layers.
  Placement strings in the model list are configuration reports, not measured
  memory consumption. Timings are observations, not a performance benchmark.

The initial scripted failure (implicit module capabilities and overwritten
startup records), initial real strict-comparison failure and a corrected
comparator unit-test error are retained in the evidence. Python production code
and dependencies were unchanged; no new Python test result is claimed here.
Candidate CI awaits an explicitly authorized push and review PR.

## Boundaries and remaining work

These are generated read-only tasks. A successful terminal-task restart does
not validate interruption/resume during inference, pause/cancel races, agentd
crash recovery while a worker is alive, or autonomous code changes. The minimal
fixture does not run capkd: daemon capability-publication calls report service
unavailable; the checks cover the spec/live grant lists and production workspace
host policy, not a complete installed capability-service deployment. Built-in
roster personas are created in the isolated home but are not task workers.

Human fresh-install/native confirmation validation stays explicitly **open**.
#247 also retains autonomous editing/refactoring, representative retrieval and
scanner evaluation, and per-member salon transport acceptance.
The separate [Akasha PR #169](https://github.com/azerothl/Akasha/pull/169) is now
merged (checked 20 October); that repository's merge alone does not establish an
installed cross-product LSP acceptance here. No release was made.
#434 and #435 remain open for additional-machine and two-GPU evidence.

Local reports and retained artifact hashes are recorded in
[the lifecycle evidence](tickets/evidence/2026-10-20-agent-lifecycle.json).
