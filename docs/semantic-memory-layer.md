# Semantic memory layer (#460)

Semantic consolidation builds inspectable interpretations above accepted Memory
V2 observations. It reuses the existing canonical source IDs and conversation
references, rather than replacing long-term facts or creating a WASM module with
access to the memory directory. No new dependency, model download or remote
inference service is required.

## Activation and review

In **Memory → Learned concepts and profiles**, choose **My profile** or an
agent. The advanced field also accepts an explicit `user:<id>` or `agent:<id>`
namespace. **Inspect** reads its concepts; **Consolidate**
processes one batch with the selected/default local chat model. Background
consolidation is off until the human enables it for that namespace.

The daemon's existing five-minute maintenance heartbeat processes another batch
while enabled. Unchanged observations are not inferred again. A stopped daemon
catches up from the persisted progress on subsequent heartbeats. No timer runs
inside the desktop UI.

Generated concepts start as candidates. Only **Confirm** allows a still-supported,
non-disputed concept into `mem.context`. **Reject** keeps the concept inspectable
and prevents the same key from being automatically reintroduced while its
evidence remains valid. A changed interpretation returns to candidate status;
contradictory evidence prevents confirmation. The UI displays source memories,
original conversation references when available, opposing evidence and the last
16 revisions. It does not display or store a model's private reasoning.

## Source graph and profiles

Each concept has a stable ID, semantic key, kind, subject namespace, timestamps,
review status, revision, evidence score, supporting observations and opposing
observations. The graph is `concept → source memory → original source/conversation`.
Kinds cover concepts, preferences, beliefs, patterns, goals, communication style,
successful workflows, recurring failures and preferred tools. The concepts of a
`user:<id>` namespace form that user's inspectable model; `agent:<id>` concepts
describe that agent's recorded outcomes. Existing `mem.object.create` can supply
accepted, sourced observations to either namespace. Arbitrary process logs are
not silently promoted to observations.

The model must cite only observation IDs included in the current batch. At least
two independent supporting observations are required. Facts from one conversation
count as one independent group; explicit source references are grouped by their
origin, and observations without an origin use a normalized text fingerprint.
The model cannot supply its own confidence or review status.

Confidence is an evidence heuristic, **not a calibrated probability**. It combines
the number of independent supporting origins with the proportion of supporting
and opposing observations, and is capped at 0.95. It does not establish that a
model interpretation is correct. Human review and inspectable source text remain
necessary.

## Bounded, persistent processing

One call selects at most 16 unprocessed observations and up to 32 additional
related observations. Existing embeddings group related facts; lexical overlap
is the fallback. Oldest pending IDs seed batches so new memories cannot starve
older ones. Existing concepts are shown only when their evidence fits in that
window. A revision must account for every previous opposing observation; older supporting evidence can move into the inspectable revision history. Each concept cites at most 32 current observations, and its history retains source fingerprints for erasure. Concepts
are never used as raw observations, preventing repeated self-reinforcement.

Sources must be accepted, non-secret, unexpired, non-document/non-narrative
objects with at most 768 characters. The existing secret filter also applies.
The response reports excluded and pending counts separately. There are at most
eight proposed concepts per call and 256 concepts per namespace. Reaching the
limit returns an error and preserves pending work.

`semantic-v1.json` in the existing memory directory stores concepts and progress
separately from `episodic.jsonl` and `objects-v2.jsonl`. A single synced temporary
file and rename commit an entire batch. Malformed/oversized output, a failed
model, timeout, changed source or concurrent review leaves that batch unconsumed.
All cited sources are checked again after inference. Changed, archived, expired,
rejected or deleted source memories invalidate dependent concepts and their
retained excerpts/history. Namespace wipe also removes its semantic profile and
background opt-in. IDs are not reused after a wipe.

The model call uses `routing=local_only`, low priority, no tools, at most 2,048
output tokens, a 32 KiB response bound and a 180-second deadline. A known inference
is cancelled on failure/timeout. This layer does not tune the model scheduler.

## API and permissions

All four intents return `MemSemanticResponse` from
`aos_proto::semantic_memory`:

| Intent | Request | Permission |
|---|---|---|
| `mem.semantic.list` | `{namespace}` | `mem.read:<namespace>` (or existing `mem.query`) |
| `mem.semantic.consolidate` | `{namespace, model_id?}` | `mem.read` and `mem.write` for that namespace |
| `mem.semantic.settings` | `{namespace, enabled}` | Existing human UI identity |
| `mem.semantic.review` | `{namespace, id, revision, accepted}` | Existing human UI identity; stale revisions refused |

Namespaces must be explicit `user:<id>` or `agent:<id>` values. An envelope with
kernel capabilities is resolved by the existing capability service; failure
does not fall back to logical grants. Non-kernel calls use scoped memory grants.
The bus and WASM host adapter share the same dispatcher and audit actions. The
audit records the actor, namespace, result and concept count without copying
source text. UI identity follows the existing local IPC trust model; this change
does not introduce multi-tenant authentication. Filesystem grants alone do not
authorize access to another subject's memory.

The ordinary agent context receives only confirmed concepts for its requested
namespace (default `user:default`) after the same scoped read authorization as
listing. Worker bootstrap/recall forwards its actual grants, and separately
retrieves its own `agent:<id>` semantic profile when authorized. The native
`mem.semantic.list` and `mem.semantic.consolidate` tools expose the same boundary;
model arguments cannot add grants. The WASM context adapter requires its
namespace memory-query capability.
No candidate/rejected/disputed concepts enter the prompt.

## Validation scope

Unit and IPC tests cover restart, provenance, scoped access, confirmation,
contradiction, failed inference, atomic refusal, bounded backlog processing,
source changes, secret exclusion and erasure. The real-model example is:

```text
cargo run -p aos-platform --no-default-features --example semantic_memory_probe --locked -- <absolute-aos-modeld> <absolute-Qwen3.5-4B-Q4_K_M.gguf>
```

It starts its own loopback broker, platform handlers and model daemon in a fresh
temporary directory and stops only the daemon it owns. Its eight observations
are synthetic. The report distinguishes actual local inference from automated
confirmation and does not claim a native UI or semantic-quality benchmark.

Representative longitudinal quality evaluation, installed native UI acceptance,
hierarchical/inter-concept merging, automatic aging and shared multi-agent
profiles remain outside this implementation. A collection of generated concepts
does not itself prove improved personalization or reasoning quality.
