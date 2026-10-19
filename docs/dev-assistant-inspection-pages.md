# Resumable project inspection (#247)

**19 October 2026 — source candidate on `codex/dev-assistant-resumable-inspection`.**
[PR #452](https://github.com/azerothl/akasha-os/pull/452) was merged as
`7cdf6ba47d39f17717034663e7e6dab08ee4d24f` after its 18 successful checks and the
expected manual CUDA skip. This follow-up is not a new Preview release.
#247, #434 and #435 remain open. The user explicitly leaves native manual
validation open.

## Continue a bounded inspection

The prior inspection stopped after its time budget and a fresh request started
over. `workspace.inspect` can now retain its pending traversal and continue:

```json
{"root":"my-project"}
```

If `continuation.available` is true, request the next page:

```json
{"root":"my-project","resume":true}
```

The same exact `fs.read:/host/<id>/**` grant is required on **every** call.
The latest saved inspection belongs to the workspace, not one actor. Any actor
with that whole-workspace read grant may continue it. A request without `resume`
starts a new inspection and replaces the previous traversal. No raw cursor,
absolute path or arbitrary offset is accepted from the caller.

The optional `page_entries` field (1–5,000) sets a smaller cooperative page
boundary, useful for background callers and reproducible checks. Omission keeps
the previous 50,000-entry traversal allowance; time slicing still applies.
Initial `max_files` / `max_bytes` limits may be lower than the host ceilings.
A continuation cannot raise, reset or change its recorded limits.

The optional reference module **0.5.0** requires **host API 5**. Existing API 1–4
modules remain supported; DeclUI contract 2 and ext-rt 1.1.0 are unchanged. The
new `dev-assistant.inspect_continue` tool supplies `resume:true` with only the
chosen workspace as input. Its native form is labelled “Continue stored
inspection” / “Poursuivre l’analyse enregistrée”. Source maps and continuation
status are also available through `dev-assistant.inventory` after a host restart.
The generic manifest still requests no host-folder permissions.

## One job budget across all pages

- Each scan page checks a ten-second deadline between traversal entries. This is
  a cooperative scan limit, not a hard end-to-end I/O latency guarantee.
- **Across the entire job:** at most 10,000 read attempts, 64 MiB of input (plus
  at most one sentinel byte), and 50,000 visited entries. The per-file 1 MiB,
  directory 20,000-entry and depth-64 limits remain.
- The pending queue is bounded to 50,000 entries / 4 MiB; the complete persisted
  metadata object is capped at 8 MiB. A queue limit is a visible coverage gap.
- A page is claimed on disk before its reads begin. If the process stops during
  the page, a policy failure aborts it, or persisting its result fails, that job
  cannot replay an uncommitted page. Cached status reports interruption and a new
  inspection is required. Normal restarts **between completed pages** can resume.
- Read attempts and consumed bytes are counted even if opening/reading fails;
  partial bytes are not dropped from accounting. A failed/uncommitted page ends
  its job; cached counters then describe its last successfully committed prefix.
- Saved continuations expire after one hour from the first page, including
  across process restarts. Changed root binding or cache-policy version refuses
  continuation and requires a new inspection.
- Calls using the same cache are serialized within one host process. This
  prevents simultaneous continuations from consuming or overwriting the same
  page twice. It is not an inter-process filesystem lock.

`scan` counters accumulate. `page_boundary` counts pauses, not missing files.
`scan.complete` becomes true only when traversal finishes **without** a recorded
coverage gap. Budget exhaustion ends the job with `complete:false` and no further
continuation. `continuation.reason` distinguishes `page_boundary`,
`coverage_limited` and `walk_complete`; it never turns a limited scan into a
complete one.

Each resumed path is checked again with the shared ignore/credential policy and
canonical identity checks. Symlinks/reparse points are not traversed. The public
result never includes the pending queue or internal binding. The host-owned
cache contains relative pending paths and eligible source metadata, not source
bodies or captured credential values. Credential-shaped/default-excluded names
are removed before enqueueing. Context-read audit errors use a fixed refusal
code, so malformed caller field names are not retained in that audit event.

## Freshness and compatibility

Earlier pages are **not re-read** during continuation. A completed traversal is
an observation across pages, not an atomic filesystem snapshot, a guarantee that
ignore rules/content remained unchanged, or a semantic symbol index. Actual
`workspace.read` requests still recheck the current file and policy. Restart the
inspection when current whole-project metadata is needed after external edits.

`content_version` hashes the accumulated source map in traversal order. On an
unchanged fixture it matches one-pass inspection. Delta comparison is available
for complete single-call scans; resumed scans explicitly report delta unavailable
rather than comparing a final map against its own partial prefix.

API 5 introduced cache policy 2. The subsequent
[content-filter quality follow-up](dev-assistant-filter-quality.md) uses policy
**3**: maps and continuations from policies 1 and 2 require a fresh inspection.
The namespace remains `source-map:v1`, the public
source-file map format. A new traversal replaces an unfinished one deliberately.

## Verification and remaining scope

[Machine-readable evidence](tickets/evidence/2026-10-19-resumable-inspection.json)
records the actual repository revision/archive hash, final executable hash,
per-page timings/counters, host restarts and local verification results.

### Observed Windows results

| Check | Result |
|---|---:|
| Platform (complete sequential suite) | 315 passed |
| Agent | 376 passed |
| Protocol | 194 passed |
| UI | 543 passed, 1 ignored |
| Strict Clippy, four crates, all targets, no embeddings | Passed |
| Python validator / signed packs | 4 / 9 passed |
| Package and WASM-host continuation after final UI cleanup | Passed (also included in platform suite) |
| Version / site documentation checks | Passed |

The archived actual repository at `7cdf6ba4` contained 1,350 files. The fixed
64-entry run finished in **26 pages with 25 host restarts**, in **9,047 ms** total;
individual calls took 49–579 ms. It read 1,329 files / 33,685,458 bytes and retained
1,258 eligible source files. Six files with known test/sample credential patterns
were withheld; no live-credential discovery is claimed.

A separate freshly extracted copy using default boundaries completed in one call
in **6,811 ms**, producing the exact same file-map fingerprint. That default run
did not encounter a real ten-second pause. These are two observations of one
repository on Windows, not a latency promise, adversarial scanner evaluation,
representative monorepo/model benchmark, or native desktop validation.

Regression checks cover file/byte/entry ceilings across pages, cache corruption,
expiry and rebinding, traversal rejection, withheld credential content/names,
concurrent continuations, a forced deadline boundary, and the packaged reference
module's WASM-to-host continuation path. The real-repository probe restarts the
host between pages, checks the cached fingerprint, and rejects an ungranted actor.

```powershell
cargo build -p aos-platform --no-default-features --example workspace_inspect_probe --locked
python scripts/verify-project-inventory.py --repo C:/path/to/repo --revision HEAD --page-entries 64 --probe C:/path/workspace_inspect_probe.exe
```

The runner copies an actual committed Git snapshot into its own new temporary
directory; it does not edit the source repository. It fails its complete-coverage
assertion when a job ends limited, while retaining the machine-readable report.
A fixed entry boundary exercises restart/resume without requiring a slow disk;
that is not a measurement of a ten-second timeout on every page. The probe itself
is capped at 128 pages / 120 seconds, independently of the host job limits.

Still open on #247: human fresh-install/native confirmations; separate Studio
LSP delivery (Akasha PR #169); autonomous agent-runtime and representative
refactor/retrieval quality; broader secret-scanner recall/false-positive evidence.
The previous real-model exercise remains a two-file fixture and is not rerun or
presented as a new model benchmark by this inspection change. #434/#435 still
require unavailable extra-machine / two-GPU evidence. CI of this source candidate
is pending publication.

[Shared project policy and prior evidence](dev-assistant-project-policy.md) ·
[Studio/DAP contracts](dev-assistant-studio.md)
