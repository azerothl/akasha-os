# Tool gate host (akasha-model Path A)

Akasha OS executes tool side effects **only after** the
[akasha-model](https://github.com/azerothl/akasha-model) authorization gate
returns `ready`, then re-checks OS permissions. The model package never runs
tools.

Upstream usage (Path C — host dispatch):
[docs/using-the-tool-gate.md](https://github.com/azerothl/akasha-model/blob/cursor/gate-host-476f/docs/using-the-tool-gate.md)
(branch `cursor/gate-host-476f` until [PR #7](https://github.com/azerothl/akasha-model/pull/7) merges).

## Flow

```text
System 2 (agent LLM) proposes tool + args
        │
        ▼
akasha-model evaluate_gate  →  ready | abstain | blocked
        │
        ▼
Akasha OS ToolHost          →  check_permissions (AgentPolicy, caps, path denylist)
        │
        ▼  only if ready + allowed
existing invoke_* / module.invoke / MCP   (same audited paths)
```

`HostOutcome.action`: `executed` | `skipped_abstain` | `skipped_blocked` | `rejected_by_host`.

## OS pieces

| Piece | Location |
|-------|----------|
| Python host + Path A | `python/aos_gate/` (`AkashaOsToolHost`, `run_os_gated_call`) |
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

Dependency: `akasha-model` from git ref `cursor/gate-host-476f` (see
`python/aos_gate/pyproject.toml`). Switch to a PyPI / main tag once PR #7 lands.

## Runtime flags

| Env | Meaning |
|-----|---------|
| `AOS_MODEL_GATE=auto` | Default: use gate when Python/`aos_gate` works; else legacy + flag |
| `AOS_MODEL_GATE=require` | Fail closed if the gate binary is missing |
| `AOS_MODEL_GATE=off` | Explicit legacy path (still logs `LEGACY_UNGATED_TOOL_PATH`) |
| `AOS_GATE_PYTHON` | Interpreter that has `aos_gate` installed |

Legacy entry points that historically skipped the gate are listed in
`python/aos_gate/src/aos_gate/legacy.py` and prefixed in tool results / agent
logs when the gate is unavailable.
