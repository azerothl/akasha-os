# Native agent project context (#247)

**19 October 2026.** [PR #454](https://github.com/azerothl/akasha-os/pull/454)
was merged as `943c16ad345630a2c931ed421c83920ab25a8607` at 12:58:33 UTC,
with 18 successful checks and the manual CUDA check skipped.
This follow-up connects the existing filtered project-context intents to the
native agent runtime. It is a source candidate, not a new Preview release.

## Activation and authority

Select these tool IDs explicitly in an agent's `tools` configuration:

```json
["workspace.inspect", "workspace.inventory", "workspace.read"]
```

The `workspace` selection prefix also selects these three native tools.
They are excluded from the permissive empty selection and unchanged default
Preview tool list. The optional Dev Assistant WASM module is not required.

Bind a workspace through the existing host workflow, then explicitly grant
the agent its intended scope through the existing capability workflow.
`workspace.inspect` and `workspace.inventory` need the exact root grant, for
example `fs.read:/host/demo/**`. `workspace.read` uses the platform's existing
per-file scope checks. Selecting tools adds no `fs.read:**` or root grant.
The existing `agent.grant` payload is `{"agent_id":"<returned ID>",
"cap":"fs.read:/host/demo/**"}`; grants remain an authorized host/operator
action. A missing grant produces a refusal, not automatic access.

The real worker already connects as `agent:<id>`. The shared bridge requires
that logical bus identity to match the executing agent ID. Caller fields such
as `actor`, `caps`, executables or endpoints are rejected; actual runtime caps
are passed in the transport to the existing platform permission boundary.
This is logical identity consistency, not new bus authentication or OS isolation.

Both native invocation entrypoints use the bridge. A salon using a shared
daemon bus client is explicitly refused, because it is not bound to that member's
identity. No impersonation or daemon-privilege fallback is added. Per-member
salon transport integration remains separate work. Existing agent tool selection,
allowlists and model-gate policy continue to apply before native invocation.

## Tool contract

| Tool | Arguments | Result |
|---|---|---|
| `workspace.inspect` | `root`; optional `resume`, `max_files`, `max_bytes`, `page_entries` | Existing bounded inspection, with an agent-sized metadata page |
| `workspace.inventory` | `root`; optional `files_offset`, `content_version` | A page of the saved source map, retaining its non-refreshed status |
| `workspace.read` | `root`, relative `path` | Existing whole-file policy check and filtered UTF-8 source, SHA-256 and policy version |

Example sequence:

```json
{"root":"demo","page_entries":64}
```

Call `workspace.inspect` again with `{"root":"demo","resume":true}` while
`continuation.available` is true. Omission of `resume` deliberately starts a new
inspection. Read `scan.complete`: traversal completion and coverage are distinct.
The platform keeps cumulative budgets, expiry, interrupted-page claims and
cache-policy-3 compatibility. No automatic retry or restart is performed by
the bridge. The 20-second transport timeout can outlive a caller while the host
finishes a page; inspect saved state before deciding how to retry.

## Metadata pagination and freshness

`files` contains at most 64 entries and 32 KiB of serialized entry metadata.
`agent_files_page` reports `offset`, `returned`, `total`, `next_offset` and
`content_version`. This pagination applies to the currently accumulated map,
independently of inspection pages. Files outside a metadata page are not absent.

For a later metadata page call `workspace.inventory`, for example:

```json
{"root":"demo","files_offset":64,"content_version":"<64-character returned hash>"}
```

A positive offset requires the original content fingerprint. A mismatch refuses
the page; start metadata pagination at offset zero again. Continue inspection
first when a complete traversal is needed, then paginate the resulting map.
Fingerprints concern source-map metadata, not an atomic filesystem snapshot.
Cached freshness, scan completeness and continuation status are preserved.

The view omits detailed secret-finding paths and delta lists to keep them out of
agent context; explicit omission flags accompany the preserved finding counts.
Full reports retain the existing host contract. Replies are capped at 512 KiB
of serialized JSON with explicit oversize errors. Source bodies are not silently
truncated. The existing 64-KiB filtered-read limit and worker prompt-compaction
rules still apply; the reply byte cap is not a model token-budget guarantee.

Fixed error codes avoid echoing host errors or malformed caller fields. Source
text remains untrusted data. A filtered-read denial does not authorize retrying
through generic `fs.read`; pre-existing broad tools/grants retain their own
contracts, and this bridge does not contain them.

## Observed verification

- Agent all-target suites: 380 library tests plus 20 binary tests passed.
- Platform library: 319 tests passed, including a real TCP broker/service test
  with 102 eligible files, continued inspection, a 64/38 metadata split,
  filtered source reads, denied secrets/traversal/ungranted access, wrong actor
  identity refusal and signed audit verification.
- Strict Clippy passed for both crates, all targets, without default features.
- An actual `aos-agent-worker` process connected to the production broker and
  platform, consumed scripted `model.infer` replies, inspected a fixture in four
  pages, read a filtered file and reported `Done`. Six planner turns plus two
  startup trace records were retained. Both source files and the withheld
  synthetic-secret file stayed unchanged; the signed audit verified.

That process test deliberately uses a deterministic model service and sets
`AOS_MODEL_GATE=off` only in the child fixture. It asserts the explicit
`LEGACY_UNGATED_TOOL_PATH` marker. It establishes process/loop/tool wiring,
not real-model choice quality, model-gate inference, agentd lifecycle, retrieval,
multi-agent editing, or human native confirmations. The fixture has no retrieval
service, so startup memory queries return their normal unavailable result.

The existing Linux Rust CI job now runs the process fixture; this candidate's
CI requires publication. Exact reports and executable/source hashes are in
[the evidence](tickets/evidence/2026-10-19-agent-context.json).

## Reproduce

```powershell
cargo test -p aos-agent --all-targets --locked -- --test-threads=1
cargo test -p aos-platform --no-default-features --lib --locked -- --test-threads=1
cargo clippy -p aos-agent -p aos-platform --no-default-features --all-targets --locked -- -D warnings
cargo build -p aos-agent --bin aos-agent-worker --locked
cargo build -p aos-platform --no-default-features --example workspace_agent_context_probe --locked
.\target\debug\examples\workspace_agent_context_probe.exe "$((Resolve-Path .\target\debug\aos-agent-worker.exe).Path)"
```

Run the built `workspace_agent_context_probe` with the absolute path of the built
`aos-agent-worker` executable. It creates a new temporary project and isolated
worker working directory, retains its logs/report and never selects a user
project. Timeout or drop kills its owned child worker. The model replies are
fixed acceptance inputs; no model or dependency is downloaded by the probe.

## Remaining acceptance

#247 stays open. Human fresh-install/native confirmation validation remains
explicitly deferred by the user. Native read-only project context now has a
worker path; real-model autonomous project/refactor workflows, representative
retrieval/refactor/scanner quality and per-member salon transport still need work.
Studio LSP delivery depends on the separate open
[Akasha PR #169](https://github.com/azerothl/Akasha/pull/169).
#434 and #435 stay open for unavailable extra-machine and two-GPU evidence.

Later acceptance: [the real-model worker follow-up](dev-assistant-real-worker.md)
uses the production model daemon and Qwen weights, fixes Windows gate interpreter
discovery and verifies a strict-gated Unicode source read. The scripted evidence
above remains its original dated record; native human validation stays open.
