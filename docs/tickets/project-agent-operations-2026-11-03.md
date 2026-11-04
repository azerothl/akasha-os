# Native project operations and retained user context — #247

[PR #476](https://github.com/azerothl/akasha-os/pull/476) was squash-merged as
`0c4855b58dd92fb321e7560e6c68c81bc24be0e1`, after its checks completed without
failure and review threads were checked. The follow-up branch starts at that
exact commit. The primary dirty checkout was preserved; local `main` was
fast-forwarded without changing its files.

## Implemented follow-up

- Native catalogue and both invocation entrypoints connect `fs.apply_patch`,
  `fs.undo_patch` and administrator-named `process.run` tasks to the existing
  project host. These tools are explicitly selected and mint no grants.
- Strict arguments reject caller identity, capabilities, commands and path
  escapes. The bridge requires an agent-bound logical bus identity. The host
  retains its patch/undo, capability, confirmation and audit checks.
- Read-only/advisory policies refuse mutations. Host offline mode and agent
  `net:deny` refuse host tasks because programs are not network-sandboxed.
  Shared salon clients cannot impersonate a member. Existing IPC authentication
  and per-member salon transport are outside this change.
- Project tool prefixes also suppress unrelated implicit module-construction
  tools. Conditional advisory questions stay advisory.
- Compaction retains the original and latest user request without duplicate
  recent turns, and replaces previous leading compaction summaries. Previously
  a tool-only recent history could lose every user message and Qwen's template
  rejected it with `No user query found in messages.`
- Model-authored runtime actions, including `agent.spawn`, must pass catalogue
  selection and the live tool allowlist before dispatch. A legacy JSON call
  previously bypassed these checks in the runtime-specific match arms. The
  existing late native/module gate and policy check remain defense in depth;
  parser `noop` diagnostics do not execute tools.

See [activation, calls and reproduction](../dev-assistant-agent-operations.md).

## Observed local validation

626 Rust tests passed: 406 agent-library tests, 24 worker/daemon tests,
194 protocol tests and two native-host integration tests. Strict Clippy passed
for the agent/protocol/platform libraries and binaries, the real-model example
and the native integration test, with CPU host features. No new CUDA build or
new inference-hardware claim is implied by those checks.

The real-broker production-host test verifies a two-file patch, a missing write
grant, a missing process grant, an explicitly denied confirmation, successful
execution after fixture approval, denied undo without write access, restoration
of both original files and the actor's audited operations. The configured task
only reruns a harmless test helper in the test executable.

Two compaction regressions were observed failing on the old implementation
before both passed: original/latest user retention over repeated compaction,
and a 2,048-token budget over tool-only continuations. Existing context-budget
and runtime tests also pass.

Real Qwen results and the latest release snapshot are retained in the
[structured evidence](evidence/2026-11-03-project-agent-operations.json).
The generated-project model runs are exploratory attempts, not a representative
quality benchmark. They preserve failures, model/worker/probe hashes and raw
critical tool responses. No model-authored file is hand-corrected by the harness.

The first three attempts failed: an incorrect Python patch, a refused relative
patch path, then a tool loop without a completed edit. All three eventually
logged the missing-user-query template error. After the compaction fix, an
attempt completed both helper extractions, ran the native `verify` task with
all 93 behavioral cases passing, and reached `Done` in 32 recorded steps.
Eleven permission-gated calls executed with zero legacy ungated steps. The
separate harness undo restored both original files and the owned model daemon
was stopped. The missing-user-query template error did not recur in that run.

That run also attempted `agent.spawn` outside its selected kit; the isolated
fixture had no agent-creation service, so no child was created. This exposed the
runtime-specific policy bypass corrected above. A direct worker regression
checks an unoffered spawn and denied runtime actions under a live allowlist.
The evidence keeps this distinction rather than treating service absence as
permission enforcement.

The final run with runtime selection enforcement refused another attempted
spawn with `hors allowlist`, created no child, retained the user query and
logged no missing-user-query template error. It nevertheless exhausted its
step budget after repeated planning and reading, without editing or running
the named task. The final source therefore has passing deterministic
integration/regression checks and a corrected observed refusal, **not reliable
autonomous refactoring acceptance**. The earlier successful edit/test/undo
workflow remains scoped to its recorded pre-enforcement worker hash. All five
attempts remain in the evidence; they are not aggregated into a success rate.

## Preview build follow-up — #444

The old [full run 41527122297](https://github.com/azerothl/akasha-os/actions/runs/41527122297)
finished with failures on all three platforms. Linux failed in Python verifier
setup (fixed by merged #473). Windows and macOS both reached Illustration Studio
and failed because its locked dependency still referenced `aos-scene` 0.18;
merged #475 updates that lockfile. Windows therefore did not need a separate
speculative code fix.

A new [full run 41545885817](https://github.com/azerothl/akasha-os/actions/runs/41545885817)
uses the exact #476 merge commit, with macOS enabled, version 0.19.0, cache reuse,
module reuse preferred, and **release creation/upload disabled**. macOS passed.
The structured snapshot records the latest Windows/Linux state; neither
installed behavior nor a release-cost improvement follows from a build result.

The separate [post-merge workspace CI 41545805962](https://github.com/azerothl/akasha-os/actions/runs/41545805962)
passed on that exact merge commit.

#439, #444, #247, #460, #434 and #435 remain open by instruction. Native UI
acceptance, representative edit/test quality, real history/retrieval quality,
additional-machine/two-GPU validation and the remaining installed pack workflows
are not accepted by this source change. The unrelated website PR #442 stays draft.
