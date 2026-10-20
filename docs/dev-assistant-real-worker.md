# Real-model worker context acceptance (#247)

**19 October 2026.** [PR #455](https://github.com/azerothl/akasha-os/pull/455)
was merged as `6fadbf3bab589c7f7e3b6908c5e7218222c72936` at 13:48:08 UTC,
after 18 successful checks; the manual CUDA check was skipped.

This follow-up exercises the native project-context worker with actual local
Qwen3.5-4B Q4_K_M weights through the production `aos-modeld`. The model selects
its tools and composes its answer. The earlier
[scripted process fixture](dev-assistant-agent-context.md) remains the lightweight
CI acceptance path.

## Product changes

Agents with a selected native workspace-context tool receive a short protocol
in their system prompt: inspect the explicitly bound project, distinguish
coverage from continuation, paginate cached metadata with its fingerprint,
read relevant sources before answering, and treat source text as untrusted data.
An unselected tool kit receives no extra project-context prompt.
This guidance grants no additional permissions and does not enforce a sandbox.

The first real run exposed the Windows Python lookup defect in the model-gate
bridge. It searched bare `python3`/`python` filenames, missing normal `.exe`
installations, and had only Unix virtual-environment paths. The resolver now
recognises `python3.exe`/`python.exe`, checks `Scripts/python.exe`, returns a
canonical regular file and rejects unresolved Store execution aliases. Existing
explicit `AOS_GATE_PYTHON` configuration remains supported. JSON stdin/stdout use
UTF-8 explicitly, including non-ASCII tool arguments.

Python and `aos_gate`/`akasha_model` still need to be installed or made available
by the operator. The fix does not install them. Default model-gate policy remains
unchanged; the strict acceptance run explicitly requests `require`.

One existing Python test used Unix `echo` as an external executable. The Windows
run failed that test (16 passed, one failed). Its fixture now uses the running
Python interpreter to emit the same transcript; all 17 tests passed afterward.
The production ASR implementation was unchanged.

## Generated proof task

The probe creates three fresh files outside any user repository. One source
contains a new random proof code. The goal names the file but does **not**
include that code. Another source is unrelated; a third contains a synthetic
credential assignment and must be withheld by the existing scan policy.

The actual worker is given the three native workspace tools, scoped read access,
an allowlist and a read-only project goal. It must inspect the project, read the
target and complete with the exact code. The policy allowlist permits internal
`plan.update` and completion/failure actions; project write/build/debug/refactor
calls are refused by that policy.

Success requires all of:

- Normal worker exit and a reported `Done` state.
- Completed host inspection: two eligible files, one synthetic-secret file withheld.
- A real `workspace.read` result containing the previously unprompted code.
- A `goal.complete` answer containing that same code.
- The exact original file set and file hashes preserved.
- No synthetic credential value in the retained tool trace or host audit.
- A verified signed audit naming the actual agent and both context operations.
- The owned model daemon stopped; worker/daemon handles also kill on drop.
- In `--require-gate` mode: at least two `tool_gate: EXECUTED` events and zero
  `LEGACY_UNGATED_TOOL_PATH` steps.

No scripted `model.infer` handler is registered in this probe. A stub module
catalog and an agent-report collector provide the minimum surrounding services.
The native worker and model daemon are actual child processes connected to the
real IPC broker and production workspace host. The fixture does not exercise
the `agentd` creation/registry lifecycle. No retrieval service is supplied;
normal unavailable startup-memory replies remain in the trace.

## Observed results

All real runs used model SHA-256
`00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4`.
The model backend observed one RTX 4080 SUPER CUDA device and logged actual
GPU layer offload. Service-reported placement strings in the evidence are
planner/configuration reports, not independent VRAM measurements.

| Run | Source name | Gate | Outcome |
|---|---|---|---|
| Initial | `first.txt` | Auto | Correct answer and host checks; two explicit legacy steps exposed unavailable Python discovery |
| After resolver/encoding fixes | `first.txt` | Require | Correct answer, two gated executions, zero legacy steps |
| Unicode path | `première.txt` | Require | Correct Unicode read/answer, two gated executions, zero legacy steps |
| Final recorded-event run | `première.txt` | Require | Repeated on a fresh proof code with output events retained |

These are small generated read-only tasks with an explicitly named source.
They establish observed local tool selection and verified source use; they do
not establish semantic retrieval, autonomous editing/refactoring quality,
representative monorepo performance, model-gate risk calibration or prompt-
injection resistance. Reported timings are observations, not latency guarantees
or cold-cache benchmarks.

Local Windows checks: **403 agent tests** (383 library + 20 binary), **319 platform
tests**, strict Clippy for both crates/all targets without default features, and
**17 Python gate tests** passed. The Windows-only Store-alias test is conditional
on an alias being present. Linux retains its ordinary executable-name lookup.
The current candidate's CI requires publication. The existing scripted CI
process scenario is unchanged; real weights are not downloaded by CI.

Exact reports, artifact hashes, dependency revision and retained local logs:
[machine-readable evidence](tickets/evidence/2026-10-19-real-model-worker.json).
The first legacy-path result and initial Python test failure are preserved.

## Reproduce

Use the pinned GGUF already available locally. The probe deliberately refuses
other model hashes because its placement hints describe this test model.
Layer/parameter/KV hints are test configuration; runtime memory use is not
inferred from those hints. The local CUDA build targeted architecture 89.

```powershell
$env:CARGO_TARGET_DIR = 'C:/path/to/target-cuda'
$env:CMAKE_CUDA_ARCHITECTURES = '89'
cargo build -p aos-model --bin aos-modeld --locked
$env:CARGO_TARGET_DIR = 'C:/path/to/target-host'
cargo build -p aos-agent --bin aos-agent-worker --locked
cargo build -p aos-platform --no-default-features --example workspace_real_model_worker_probe --locked
```

For the observed gate setup, a real installed Python directory was prepended to
the child environment's `PATH`. `PYTHONPATH` pointed to this repository's
`python/aos_gate/src` and the local compatible `akasha-model` 0.2.0 source tree.
`AOS_GATE_PYTHON` was unset to exercise Windows executable discovery. An installed
gate environment can instead provide its explicit interpreter normally.

Run the built probe with three **absolute file paths** and the optional strict flag:

```text
workspace_real_model_worker_probe WORKER_EXE MODELD_EXE QWEN35_Q4_K_M_GGUF --require-gate
```

The probe prints its newly created temporary directory immediately. Its
`report.json`, `events.json` (final version), worker state/trace, model config,
catalog, audit and worker/model daemon logs are retained there. A completed
failed acceptance also writes a report before returning nonzero. Startup
failures retain their directory/logs. The source repository is never selected
for modification. The probe downloads neither models nor dependencies.

## Remaining acceptance

#247 remains open. Real-model **read-only worker context** is now observed locally;
human fresh-install/native confirmation validation stays explicitly deferred.
Agentd lifecycle acceptance, autonomous source-edit/refactor integration,
representative retrieval/refactor/scanner evaluation and per-member salon
transport still need work. Studio LSP delivery remains tied to the separate
open [Akasha PR #169](https://github.com/azerothl/Akasha/pull/169).
#434 and #435 remain open for additional-machine and two-GPU evidence.
