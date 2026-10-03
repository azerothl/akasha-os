# Tool gate host (akasha-model Path A + D)

Akasha OS executes tool side effects **only after** the
[akasha-model](https://github.com/azerothl/akasha-model) authorization gate
returns `ready`, then re-checks OS permissions. The model package never runs
tools.

Upstream usage:

- Path C (host dispatch): [using-the-tool-gate.md](https://github.com/azerothl/akasha-model/blob/main/docs/using-the-tool-gate.md)
- Path D (outcomes): same doc, *Outcomes* section — OS appends compatible JSONL

## Flow

```text
System 2 (agent LLM) proposes tool + args (+ provenance trust tier)
        │
        ▼
akasha-model evaluate_gate  →  ready | abstain | blocked
        │
        ▼
Akasha OS ToolHost          →  credential policy, source-trust,
                               AgentPolicy, caps, path denylist
        │
        ▼  only if ready + allowed
existing invoke_* / module.invoke / MCP   (vault injects secrets here)
        │
        ▼
Path D outcomes JSONL  (+ optional HITL review on escalate stand-in)
```

`HostOutcome.action`: `executed` | `skipped_abstain` | `skipped_blocked` | `rejected_by_host`.

## OS pieces

| Piece | Location |
|-------|----------|
| Python host + Path A/D | `python/aos_gate/` (`AkashaOsToolHost`, `run_os_gated_call`) |
| Outcomes logger | `python/aos_gate/src/aos_gate/outcomes_log.py` |
| Source-trust | `python/aos_gate/src/aos_gate/trust.py` (+ Rust mirror) |
| HITL escalate | `python/aos_gate/src/aos_gate/hitl.py` |
| Credentials OOB | `python/aos_gate/src/aos_gate/credentials.py` |
| MCP → ToolSpec | `python/aos_gate/src/aos_gate/mcp_catalog.py` |
| Catalogue → `ToolSpec` | `python/aos_gate/src/aos_gate/catalog.py` |
| Rust bridge + UI logs | `crates/aos-agent/src/tool_gate.rs` |
| Agent loop | `aos-agent-worker` `execute_action` |
| Room turns | `tool_exec::execute_room_tool` |

## Install / test

```sh
python3 -m venv .venv-aos-gate && source .venv-aos-gate/bin/activate
pip install -e 'python/aos_gate[dev]'
pytest python/aos_gate/tests -q
# Rust dispatch unit tests (no Python required):
cargo test -p aos-agent tool_gate -- --nocapture
```

Dependency: `akasha-model` pinned to git tag `v0.2.0` (host + outcomes;
[akasha-model#10](https://github.com/azerothl/akasha-model/issues/10)).

## Runtime flags

| Env | Meaning |
|-----|---------|
| `AOS_MODEL_GATE=auto` | Default: use gate when Python/`aos_gate` works; else legacy + flag |
| `AOS_MODEL_GATE=require` | Fail closed if the gate binary is missing |
| `AOS_MODEL_GATE=off` | Explicit legacy path (still logs `LEGACY_UNGATED_TOOL_PATH`) |
| `AOS_GATE_PYTHON` | Interpreter that has `aos_gate` installed |
| `AOS_GATE_OUTCOMES=auto\|on\|off` | Path D JSONL append (default auto=on) |
| `AOS_GATE_OUTCOMES_PATH` | Outcomes JSONL path (default `var/gate/outcomes.jsonl`) |
| `AOS_GATE_REVIEWS_DIR` | HITL review records (default `var/gate/reviews`) |

Legacy entry points that historically skipped the gate are listed in
`python/aos_gate/src/aos_gate/legacy.py` and prefixed in tool results / agent
logs when the gate is unavailable.

---

## Path D — production outcomes

After each gated attempt (Python `run_os_gated_call` or Rust
`decide_gated_tool*`), OS appends a redacted
`GateOutcomeRecord`-compatible JSONL row via `akasha_model.outcomes`.

```sh
# Offline summarize + threshold *suggestions* (never auto-applied)
python -m aos_gate.cli summarize --request - <<'JSON'
{"outcomes_path": "var/gate/outcomes.jsonl"}
JSON
```

Rules:

- Populate `success`, `user_forced`, plan status/reason, and `source_trust=…` in notes when known.
- Credential-shaped argument values are redacted before persist.
- **Never** auto-write upstream `DEFAULT_*` or silent OS policy from suggestions.

---

## Source-trust / authority confusion (AIRGuard-style)

Untrusted context (retrieved docs, web, MCP tool output) may inform the
agent's *proposal*, but must not become the *authorization* for side effects.

| Tier | High-impact tools (irreversible / confirmation / write-delete) |
|------|----------------------------------------------------------------|
| `trusted` | Allowed subject to normal host checks |
| `mixed` | Requires explicit human confirmation |
| `untrusted` | **Rejected** (`rejected_by_host`) — escalate for HITL |

Enforcement lives in OS (`AkashaOsToolHost.check_permissions` and Rust
`trust_allows_host_execution`). Path A signals may soften `authorized` /
raise `confirmation_needed`, but the host is authoritative. Prompt injection
into the gate subprocess cannot bypass the Rust/Python permission re-check
on the live executor path.

Pass `context.source_trust` from the bridge (`trusted` \| `mixed` \| `untrusted`).

ASR / OS STT transcripts are **untrusted** (or **mixed** if the operator also
typed). Overlay: `aos_gate.asr.overlay_asr_transcript` when the evaluate
request includes `asr_transcript`. Host intents: `speech.ingest_transcript`
/ `speech.transcribe` (opt-in `AOS_ASR_COMMAND`, **no** bundled weights).
See [speech-to-gate.md](speech-to-gate.md).

---

## HITL escalate (approve / deny)

Until akasha-model exposes a first-class `escalate` status
([model#18](https://github.com/azerothl/akasha-model/issues/18)), OS treats as
escalate stand-in:

- `GateContext.needs_human_review`
- `abstain` with elevated risk / confirmation
- host rejection citing authority confusion / untrusted provenance

Flow:

1. Create durable review under `var/gate/reviews/<id>.json`
2. **Deny** → never execute; log Path D row
3. **Approve** → re-enter `evaluate_gate` with `confirmation_given` + trusted
   provenance + `user_forced` audit; execute only if ready + `check_permissions`

```sh
python -m aos_gate.cli review-list --request - <<<'{"status":"pending"}'
python -m aos_gate.cli review-deny --request - <<<'{"review_id":"…"}'
python -m aos_gate.cli review-approve --request - <<<'{"review_id":"…","host":{"actor_caps":["notes:**"]}}'
```

No silent auto-approve from model threshold suggestions.

---

## Out-of-band credentials

- Proposals must not include credential-shaped keys (`api_key`, `token`,
  `authorization`, …). Host rejects them when `enforce_credential_policy` is on.
- `CredentialVault` injects secrets only inside `AkashaOsToolHost.execute`.
- Outcomes / reviews persist redacted arguments only.

MCP server env already supports `${secret:name}` interpolation in
`crates/aos-agent/src/mcp.rs`; tool-argument secrets follow the same rule:
never in the proposal.

---

## MCP → ToolSpec

```python
from aos_gate import tools_from_mcp_manifest

tools = tools_from_mcp_manifest(mcp_tools_list_json, server="demo")
# names: mcp.demo:<tool> ; caps: mcp.use:demo
# destructive/ambiguous names → requires_confirmation + irreversible
```

CLI: pass `mcp_manifest` (and optional `mcp_server`) instead of `catalog`.

Known gaps: see `aos_gate.mcp_catalog.MCP_MAPPING_GAPS` (no first-class MCP
caps; annotations not yet wired; fail closed on write/delete-like names).
