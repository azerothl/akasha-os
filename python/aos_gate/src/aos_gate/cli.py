"""JSON CLI bridge: Rust agent ↔ aos_gate Path A (akasha-model).

Modes
-----
``evaluate``
    Run ``evaluate_gate`` only; print plan JSON. Rust then mirrors
    ``dispatch_plan`` with live OS ``check_permissions`` / ``execute``.

``dispatch``
    Full ``run_gated_call`` with ``AkashaOsToolHost``. ``execute`` is a
    recording stub unless ``executor`` is ``echo`` (default) — used by
    tests and demos. Live side effects stay in the Rust worker after a
    ready plan (evaluate mode).
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

from akasha_model import ToolProposal, describe_plan, evaluate_gate

from .catalog import tools_from_os_catalog
from .host import AkashaOsToolHost, RecordingExecutor
from .path_a import outcome_to_dict, run_os_gated_call
from .signals import context_from_mapping, signals_from_context


def _load_request(path: str | None) -> dict[str, Any]:
    raw = sys.stdin.read() if path in (None, "-", "") else open(path, encoding="utf-8").read()
    data = json.loads(raw)
    if not isinstance(data, dict):
        raise SystemExit("request JSON must be an object")
    return data


def _proposal(data: dict[str, Any]) -> ToolProposal:
    prop = data.get("proposal") or {}
    name = prop.get("tool_name") or prop.get("name")
    if not name:
        raise SystemExit("proposal.tool_name required")
    return ToolProposal(str(name), prop.get("arguments"))


def cmd_evaluate(data: dict[str, Any]) -> dict[str, Any]:
    tools = tools_from_os_catalog(data.get("catalog") or data.get("tools") or [])
    proposal = _proposal(data)
    ctx = context_from_mapping(data.get("context"))
    signals = signals_from_context(ctx)
    plan = evaluate_gate(tools, proposal, signals)
    return {
        "mode": "evaluate",
        "describe": describe_plan(plan),
        "plan": {
            "status": plan.status,
            "tool_name": plan.tool_name,
            "arguments": plan.arguments,
            "reason": plan.reason,
            "executable": plan.executable,
            "choice_probability": plan.choice_probability,
            "choice_confidence": plan.choice_confidence,
            "risk_score": plan.risk_score,
        },
    }


def cmd_dispatch(data: dict[str, Any]) -> dict[str, Any]:
    tools = tools_from_os_catalog(data.get("catalog") or data.get("tools") or [])
    proposal = _proposal(data)
    host_cfg = data.get("host") or {}
    tool_capabilities = {
        name: spec.required_capability for name, spec in tools.items()
    }
    allow = host_cfg.get("tool_allowlist")
    host = AkashaOsToolHost(
        executor=RecordingExecutor(),
        actor_caps=set(host_cfg.get("actor_caps") or []),
        net_deny=bool(host_cfg.get("net_deny", False)),
        fs_write=bool(host_cfg.get("fs_write", True)),
        tool_allowlist=set(allow) if isinstance(allow, list) else None,
        tool_capabilities=tool_capabilities,
    )
    ctx = context_from_mapping(data.get("context"))
    outcome = run_os_gated_call(tools, proposal, host, context=ctx)
    payload = outcome_to_dict(outcome)
    payload["mode"] = "dispatch"
    payload["executor_log"] = host.executor.log  # type: ignore[attr-defined]
    return payload


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(prog="aos-gate", description=__doc__)
    parser.add_argument(
        "command",
        choices=("evaluate", "dispatch"),
        help="evaluate = plan only; dispatch = run_gated_call with OS host",
    )
    parser.add_argument(
        "--request",
        default="-",
        help="JSON request path (default: stdin)",
    )
    args = parser.parse_args(argv)
    data = _load_request(args.request)
    if args.command == "evaluate":
        out = cmd_evaluate(data)
    else:
        out = cmd_dispatch(data)
    json.dump(out, sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
