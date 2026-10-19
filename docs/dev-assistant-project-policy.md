# Project context policy and acceptance (#247)

**19 October 2026 — baseline merged through [PR #452](https://github.com/azerothl/akasha-os/pull/452).**
The measurements below are the dated API-4 record. The [resumable inspection
follow-up](dev-assistant-inspection-pages.md) defines API 5 / reference 0.5.0;
it does not overwrite those earlier observations.
The previous DAP/Studio/subtree slice was merged through
[PR #451](https://github.com/azerothl/akasha-os/pull/451), commit
`1c525d29b8a94c5d032d72d85c7a7daec929c253`, at 03:27:47 UTC, with 18 successful
checks and the manual CUDA job skipped. This follow-up is not a new release.
#247, #434 and #435 remain open.

## Explicit project context

The reference module 0.4.0 requires **host API 4**, UI contract 2 and ext-rt 1.1.0.
API 1–3 modules remain supported. Its generic manifest still requests no host
access: bind the directory, declare the chosen exact capabilities, repackage,
and approve them through the normal install/update review.

| Intent / reference tool | Capability | Behaviour |
|---|---|---|
| `workspace.inspect` / `dev-assistant.inspect` | Exact `fs.read:/host/<id>/**` | Scan the bound project, report exclusions/limits, persist a source-file map in the host session directory |
| `workspace.inventory` / `dev-assistant.inventory` | Same root grant | Read that map, explicitly marked **not rechecked**; refuse a changed binding or policy version |
| `workspace.read` / `dev-assistant.read_safe` | `fs.read` matching the requested file | Return an existing eligible UTF-8 source, its logical path and SHA-256; reject the whole file if a supported credential pattern is detected |

Requests use the bound workspace ID (or exact `/host/<id>` root). A safe read
also requires a relative `path`. It is limited to 64 KiB of returned source;
the filter checks the complete file rather than leaking a safe-looking prefix
before a secret further down. Denied reads never return source snippets.

Inspection accepts optional lower `max_files` / `max_bytes` limits. The native
forms expose scan, cached map and filtered source read; the read-only preview
always uses the latest tool result. Reading a cached map does not refresh it.

## One policy for automatic discovery

The same native policy is used by inspection, filtered reads, source search,
and subtree proposal validation. Root and nested `.gitignore` / `.ignore` rules
apply even without a `.git` directory. Default exclusions include `.git`,
`node_modules`, `target`, `dist`, Python environments/caches, `.env` / `.env.*`, common
credential/key filenames and certificate/private-key suffixes. Symlinks and
Windows reparse points are not traversed. Explicit file reads require regular
files, excluding sockets and pipes.

`fs.search` / `code.search` now use one native literal-text engine instead of
separate ripgrep/fallback paths. Query text is never an executable option.
Subdirectory searches preserve their full `/host/<id>/<subdir>/...` paths.
Files containing a recognized credential on another line are withheld as a
whole. Search audit events retain query size and a redaction flag, not query text.
Existing result limits remain at 200, with truncation reported explicitly.

Recognized patterns are deliberately limited: private-key headers, selected
credential-shaped token prefixes/access-key identifiers, and long literal
assignments to selected password/token/key fields. Reports contain only the
relative path, rule name and line number (up to eight findings per file and
256 file reports). Credential-shaped filenames are excluded without returning
the filename. Obvious placeholder assignments are allowed. Test keys and sample
credentials may be withheld too: a match is **not proof of a live credential**.

This is not a complete secret scanner or DLP boundary. Obfuscated/unknown formats
can be missed. Generic privileged `fs.read`, debug variable inspection and the
separate Studio daemon retain their own capability/policy contracts; these
filters do not sandbox those services or remove already-granted broad rights.
Subtree proposal checks apply the supported content patterns to both the current
and proposed text before retaining before-images.

## Bounds and durable metadata

A scan is bounded to 10,000 read attempts, 64 MiB of input (plus at most one
sentinel byte), 1 MiB per file, 50,000 entries, depth 64 and a ten-second deadline.
Binary bytes count against the scan budget. A directory with over 20,000 entries
is reported as incomplete. Each ignore file is limited to 64 KiB, an inherited
ignore chain to 128 KiB; the compiled matcher cache is bounded. Limits or missing
input are visible in `scan.complete` / `scan.skipped`; an incomplete scan must
not be presented as complete coverage.

Metadata is stored atomically at `sessions/workspace-inventory-<id>.json`, capped
at 8 MiB, under namespace `workspace:<id>:source-map:v1`. It stores eligible file
paths, byte/line counts, extension and whole-file SHA-256, plus policy/scan metadata.
It stores **no source contents or captured credential values**. The `language`
field is the extension label, not a parser-derived semantic classification.

`content_version` fingerprints the eligible source map. After complete scans,
`delta` distinguishes added/changed paths and paths no longer eligible. That last
category can reflect deletion **or exclusion**, not necessarily a deleted disk
file. Delta is unavailable for partial scans or changed bindings/policies.
The map is observed during a scan, not an atomic filesystem snapshot, code-symbol
index or structural embedding database. Code Studio still owns its existing
semantic index and retrieval contract.

## Observed acceptance

[Machine-readable evidence](tickets/evidence/2026-10-19-project-acceptance.json)
records exact source-map counts, timings, model identity, binary hashes and checks.

- The native host inspected a committed snapshot of the actual Akasha OS repository read-only, performed
  literal source search, rejected an ungranted actor and reloaded the map after
  host restart. Source files were not modified. Known test credential/key patterns
  in this repository were withheld; these are not reported as discovered live secrets.
- Qwen3.5-4B Q4_K_M authored two separate Python helper-extraction proposals.
  Each was submitted under its own scoped actor identity, reviewed, denied once
  without writes, then applied through the confirmed production host path.
  **93 behaviour cases** passed. Duplicate definitions were refused and the validator
  checked that public functions actually reach a shared extracted helper. Undo restored the
  exact originals and the signed audit chain verified.
- An initial Qwen candidate passed behaviour cases but retained duplicate function
  definitions. Source review exposed this; the validator was strengthened and
  the model corrected its candidate after explicit feedback. No source cleanup
  was substituted by the harness.
- An earlier Spark-X2.5-4B Q4_K_M run produced an invalid backend candidate twice, including one
  syntax-feedback retry. Those candidates were rejected before application; no
  hand-written correction was substituted. This failure is part of the evidence.

The model scenario uses a generated two-file project, a fixed pure-Python validator
and explicit fixture-only automated approvals. Generated code is checked against
an AST/call allowlist, executed with a restricted builtins namespace and a bounded
host task. Requests, raw model responses, proposals, review and logs remain in
its newly generated fixture. It is not an autonomous `agentd` run, a representative
monorepo refactor, or evidence of concurrent inference on multiple GPUs.

### Local Windows checks

The final commands and their exact results are retained in the JSON evidence.
The checks include the complete platform/agent/protocol/UI suites, strict Clippy,
the reference module's WASM-to-host calls, and the generated-code validator.
Those four validator tests are configured to run on the three CI operating
systems without a model; CI of this candidate is still pending.
A generated NTFS junction was actually created on this PC: discovery skipped it,
explicit context reads refused it, and its external target stayed unchanged.
Unix special-file coverage is committed but has not been run locally on Linux.

| Check on Windows | Result |
|---|---:|
| Platform (complete sequential suite) | 306 passed |
| Agent | 376 passed |
| Protocol | 194 passed |
| UI | 543 passed, 1 ignored |
| Strict Clippy, four crates, all targets, no embeddings | Passed |
| Model example Clippy (CPU configuration) | Passed |
| Generated-code validator | 4 passed |
| Signed module pack contracts | 9 passed |
| Version and site documentation checks | Passed |

The actual repository corpus is a `git archive` of merged commit
`1c525d29b8a94c5d032d72d85c7a7daec929c253`: 1,339 archived files, 1,318 files read,
33,567,080 input bytes and 1,248 eligible text files. Five files containing known
test/sample credential patterns were withheld; 65 binaries, two oversized files
and three ignored paths were skipped. The final binary's first scan of a fresh
copy hit the deadline at **10,043 ms**, after 310 eligible files (6,683,604 bytes).
A **repeat on that same copy** completed in **7,105 ms**, with the complete counts
above and the same content fingerprint as an earlier full scan. Cache reload and
capability denial passed in both observations. These timings are not a latency
promise: dependable complete cold coverage within this fixed deadline remains
open. The evidence retains earlier partial runs too; no partial result is called
complete. The wrapper retains a machine-readable report before failing its
complete-coverage assertion.

A full-suite run also exposed an old test helper that reused stores after Windows
reused a process ID. The helper now creates an exclusive random directory; the
complete platform suite then passed. No production memory behaviour was changed.

## Reproduce

```powershell
cargo build -p aos-platform --no-default-features --example workspace_inspect_probe --locked
python scripts/verify-project-inventory.py --repo C:/path/to/repo --revision HEAD --probe C:/path/workspace_inspect_probe.exe
cargo build -p aos-llama --example project_proposal --locked
cargo build -p aos-platform --no-default-features --example workspace_model_refactor_probe --locked
python scripts/verify-project-model.py --model C:/path/model.gguf --model-probe C:/path/project_proposal.exe --host-probe C:/path/workspace_model_refactor_probe.exe
```

The inspection output must be a new absolute directory outside the inspected
repository. The model runner creates its own marked temporary project and does
not download a model or dependency. Run it with an accessible installed Python;
Windows Store execution aliases are refused during native context preparation.
The runner never selects a user project for edits.
After application, the runner requests reviewed undo even if native verification
fails, then checks the originals. Normal undo conflict safeguards still apply.

## Remaining acceptance

- Human-operated freshly installed native module flow and confirmations on target
  OSes; automated WASM/headless rendering do not establish that manual result.
  The user explicitly chose to leave this validation open on 19 October.
- Compatible Studio LSP delivery, tracked by the separate Akasha PR #169; no
  upstream PR was merged by this task.
- Autonomous agent-runtime integration and refactor/retrieval quality on larger,
  representative projects; the two-file model exercise does not establish these.
- Scanner recall/false-positive evaluation on a broader non-sensitive corpus,
  plus any future stronger process/filesystem isolation as a separate contract.
- Dependable complete cold coverage within the fixed scan budget; the latest
  fresh-copy scan was partial even though its repeat completed.
- CI of this new source candidate. Extra-machine cohort and two-GPU evidence
  remain explicitly open in #434/#435.
