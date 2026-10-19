# Project content filter: synthetic evaluation (#247)

**19 October 2026.** Resumable inspection was merged through
[PR #453](https://github.com/azerothl/akasha-os/pull/453), commit
`0f980f27327218f677e83012740b6dbda17e76e9`. This follow-up evaluates and improves
the shared project content policy. It is not a release or completion of #247.

## Observed defects and changes

The previous assignment detector examined the first `=` (or first `:`) only.
It missed a sensitive member after another JSON member, a later assignment on
the same line, and a JSON value containing `=`. Authorization values containing
`Bearer`/`Basic` and a space were also missed.

The line-oriented lexer now visits separators outside quoted strings, checks
each supported assignment name and recognises those authorization schemes.
Escaped quotes do not end strings prematurely. Values inside unrelated source
strings are not reinterpreted as assignments. Type annotations and source
expressions are not treated as literals merely because their first identifier
is long. Unquoted field values stop at their delimiter. The existing token and
private-key rules, eight findings per file and redacted reports remain in use.

Discovery, filtered source reads, source search and subtree proposal validation
share this production policy. It is still a heuristic, not a language parser,
complete secret scanner, DLP boundary or process/filesystem sandbox.

Cache policy is **3**. Both cached maps and resumptions produced under policies
1 or 2 are refused; explicitly start `workspace.inspect` again. This prevents
continuing earlier unfiltered pages under the new rules. Host API 5, reference
0.5.0, UI contract 2 and ext-rt 1.1.0 are unchanged.

## Corpus and measured limits

The [74-case corpus](../fixtures/workspace-policy-quality.json) contains only
synthetic examples. [Its labeling contract](../fixtures/README.md) distinguishes
ground truth from expected current behaviour, including known errors.
The same fixed corpus was run before and after the change.

| Result | Baseline | Follow-up |
|---|---:|---:|
| Sensitive cases detected | 19 / 47 | 40 / 47 |
| Sensitive cases missed | 28 | 7 |
| Benign cases withheld (false positives) | 2 / 27 | 2 / 27 |
| Benign cases retained | 25 | 25 |
| Precision on this corpus | 90.48% | 95.24% |
| Recall on this corpus | 40.43% | 85.11% |

The seven recorded misses cover short secrets, an unsupported camel-case key,
concatenation, a value on another line, lower-case/split private-key headers and
an unknown credential field. The two recorded false positives are a long
unquoted source identifier assigned to `token` and a token-shaped example in
prose. Neither known misses nor false positives are hidden by regression checks.
Encoded, obfuscated, computed or unrecognised credentials can still escape.

These are development regression cases, with related assignment variants and
no held-out evaluation or independent annotation. These percentages apply only
to this corpus. They do not establish general real-project scanner quality.
No real secret was used, and no external detector/service was called.

## Verification and remaining work

The native evaluator reports only case IDs, labels, finding rules/line numbers,
confusion counts and the corpus hash. A platform test additionally writes every
case into an isolated temporary directory, checks actual filtered reads, scans
the directory and verifies withheld values are absent from reports. Separate
tests cover quoted text, Unicode, long prefixes and old cache rejection.
The evaluator is added to the existing Linux Rust CI job.

Exact results and fingerprints are recorded in
[the evidence](tickets/evidence/2026-10-19-filter-quality.json).

Observed on this Windows host: **318 platform tests passed**, including actual
read/discovery checks for all 74 corpus inputs and policy-1/2 cache rejection.
Strict Clippy passed for all platform targets without default features. The
standalone evaluator passed `--check` on the final implementation.

A committed snapshot of the actual repository (`0f980f2`, 1,352 archived files)
completed in 26 pages with 25 host restarts, in 9,814 ms across page calls:
1,331 files / 33,747,824 bytes read, 1,260 eligible sources, six files containing
known test/sample credential patterns withheld. A fresh extraction with default
page boundaries completed in one page in 6,882 ms, yielding the same source-map
fingerprint. These are observations, not latency promises or real-secret
discovery claims. The forced run used 64-entry pages, not actual deadline hits.

CI of this follow-up requires publication. No native human confirmation,
autonomous model run, retrieval benchmark or release is claimed.

#247 remains open for human fresh-install/native confirmation validation
(explicitly deferred by the user), autonomous agent integration and representative
refactor/retrieval/scanner quality. Studio LSP delivery still depends on the
separate open [Akasha PR #169](https://github.com/azerothl/Akasha/pull/169).
#434 and #435 remain open for unavailable additional-machine/two-GPU evidence.

Later source work: [native worker context](dev-assistant-agent-context.md) adds
explicit native agent routing to this policy, with an actual-process scripted
acceptance fixture. The scanner measurements above remain a dated record.
