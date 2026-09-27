"""JSON CLI bridge: Rust agent ↔ aos_gate Path A (akasha-model).

Modes
-----
``evaluate``
    Run ``evaluate_gate`` only; print plan JSON. Rust then mirrors
    ``dispatch_plan`` with live OS ``check_permissions`` / ``execute``.

``dispatch``
    Full ``run_gated_call`` with ``AkashaOsToolHost``. ``execute`` is a
    recording stub; vault secrets from the request are injected at execute.
    Live side effects stay in the Rust worker after a ready plan (evaluate).

``record``
    Append one Path D outcome row from a HostOutcome-shaped JSON payload
    (used by the Rust bridge after evaluate + OS dispatch).

``summarize``
    Print offline ``summarize_outcomes`` + ``suggest_threshold_updates``
    (suggestions never applied).

``review-create`` / ``review-list`` / ``review-approve`` / ``review-deny``
    HITL escalate stand-in loop.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

from akasha_model import ToolProposal, ToolSpec, describe_plan, evaluate_gate
from akasha_model.host import HostOutcome
from akasha_model.tool_calling import ToolCallPlan

from .catalog import tools_from_os_catalog
from .credentials import CredentialVault
from .hitl import (
    approve_review,
    create_review,
    deny_review,
    list_reviews,
    load_review,
)
from .host import AkashaOsToolHost, RecordingExecutor
from .mcp_catalog import tools_from_mcp_manifest
from .outcomes_log import OutcomeLogConfig, dump_summary_json, record_from_mapping
from .path_a import outcome_to_dict, run_os_gated_call
from .signals import GateContext, context_from_mapping, signals_from_context
from .trust import parse_source_trust


def _load_request(path: str | None) -> dict[str, Any]:
    if path in (None, "-", ""):
        raw = sys.stdin.read()
        if not raw.strip():
            return {}
    else:
        raw = open(path, encoding="utf-8").read()
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


def _tools_from_request(data: dict[str, Any]) -> dict[str, ToolSpec]:
    if data.get("mcp_manifest") is not None:
        return tools_from_mcp_manifest(
            data["mcp_manifest"],
            server=data.get("mcp_server"),
        )
    return tools_from_os_catalog(data.get("catalog") or data.get("tools") or [])


def _host_from_request(
    data: dict[str, Any], tools: dict[str, ToolSpec],
) -> AkashaOsToolHost:
    host_cfg = data.get("host") or {}
    tool_capabilities = {
        name: spec.required_capability for name, spec in tools.items()
    }
    allow = host_cfg.get("tool_allowlist")
    vault = None
    vault_cfg = host_cfg.get("credential_vault") or data.get("credential_vault")
    if isinstance(vault_cfg, dict):
        vault = CredentialVault(
            secrets={
                str(k): {str(sk): str(sv) for sk, sv in v.items()}
                for k, v in (vault_cfg.get("secrets") or {}).items()
                if isinstance(v, dict)
            },
            global_secrets={
                str(k): str(v)
                for k, v in (vault_cfg.get("global_secrets") or {}).items()
            },
        )
    ctx = context_from_mapping(data.get("context"))
    return AkashaOsToolHost(
        executor=RecordingExecutor(),
        actor_caps=set(host_cfg.get("actor_caps") or []),
        net_deny=bool(host_cfg.get("net_deny", False)),
        fs_write=bool(host_cfg.get("fs_write", True)),
        tool_allowlist=set(allow) if isinstance(allow, list) else None,
        tool_capabilities=tool_capabilities,
        enforce_credential_policy=bool(
            host_cfg.get("enforce_credential_policy", True)
        ),
        credential_vault=vault,
        source_trust=ctx.source_trust,
        confirmation_given=ctx.confirmation_given,
        tools=dict(tools),
    )


def cmd_evaluate(data: dict[str, Any]) -> dict[str, Any]:
    tools = _tools_from_request(data)
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
        "context": {
            "source_trust": ctx.source_trust.value,
            "needs_human_review": ctx.needs_human_review,
        },
    }


def cmd_dispatch(data: dict[str, Any]) -> dict[str, Any]:
    tools = _tools_from_request(data)
    proposal = _proposal(data)
    host = _host_from_request(data, tools)
    ctx = context_from_mapping(data.get("context"))
    outcome = run_os_gated_call(
        tools,
        proposal,
        host,
        context=ctx,
        success=data.get("success"),
        user_forced=bool(data.get("user_forced", False)),
        notes=str(data.get("notes") or ""),
        hitl_entrypoint=str(data.get("entrypoint") or "aos_gate.cli:dispatch"),
        open_hitl_on_escalate=bool(data.get("open_hitl_on_escalate", True)),
    )
    payload = outcome_to_dict(outcome)
    payload["mode"] = "dispatch"
    payload["executor_log"] = host.executor.log  # type: ignore[attr-defined]
    payload["source_trust"] = ctx.source_trust.value
    return payload


def cmd_record(data: dict[str, Any]) -> dict[str, Any]:
    cfg = OutcomeLogConfig.from_env(data.get("outcomes_path"))
    payload = dict(data)
    if data.get("source_trust"):
        trust = parse_source_trust(data["source_trust"]).value
        notes = str(payload.get("notes") or "")
        tag = f"source_trust={trust}"
        if tag not in notes:
            payload["notes"] = f"{notes}; {tag}".strip("; ").strip()
    record = record_from_mapping(payload, config=cfg)
    return {
        "mode": "record",
        "recorded": record is not None,
        "path": str(cfg.path),
        "record": record.to_json() if record is not None else None,
    }


def cmd_summarize(data: dict[str, Any]) -> dict[str, Any]:
    path = data.get("outcomes_path") or data.get("path")
    summary = json.loads(dump_summary_json(path))
    summary["mode"] = "summarize"
    return summary


def cmd_review_create(data: dict[str, Any]) -> dict[str, Any]:
    plan_data = data.get("plan") or {}
    plan = ToolCallPlan(
        str(plan_data.get("status") or "abstain"),
        plan_data.get("tool_name") or data.get("tool_name"),
        plan_data.get("arguments") or data.get("arguments"),
        str(plan_data.get("reason") or data.get("reason") or "escalate"),
    )
    action = str(data.get("action") or "skipped_abstain")
    if action not in {
        "executed", "skipped_abstain", "skipped_blocked", "rejected_by_host",
    }:
        action = "skipped_abstain"
    outcome = HostOutcome(
        action,  # type: ignore[arg-type]
        plan,
        None,
        str(data.get("host_reason") or plan.reason),
    )
    ctx = context_from_mapping(data.get("context"))
    if data.get("force") or data.get("needs_human_review"):
        ctx = GateContext(
            has_required_capability=ctx.has_required_capability,
            policy_allows=ctx.policy_allows,
            sufficient_context=ctx.sufficient_context,
            confirmation_given=ctx.confirmation_given,
            risk_level=ctx.risk_level,
            confirmation_needed=ctx.confirmation_needed,
            source_trust=ctx.source_trust,
            needs_human_review=True,
        )
    review = create_review(
        outcome,
        context=ctx,
        entrypoint=str(data.get("entrypoint") or ""),
        notes=str(data.get("notes") or ""),
        reviews_dir=data.get("reviews_dir"),
    )
    return {"mode": "review-create", "review": review.to_json()}


def cmd_review_list(data: dict[str, Any]) -> dict[str, Any]:
    status = data.get("status", "pending")
    if status == "all":
        status = None
    rows = list_reviews(status=status, reviews_dir=data.get("reviews_dir"))
    return {
        "mode": "review-list",
        "reviews": [r.to_json() for r in rows],
    }


def cmd_review_deny(data: dict[str, Any]) -> dict[str, Any]:
    review_id = data.get("review_id")
    if not review_id:
        raise SystemExit("review_id required")
    review = deny_review(
        str(review_id),
        resolver=str(data.get("resolver") or "operator"),
        notes=str(data.get("notes") or ""),
        reviews_dir=data.get("reviews_dir"),
    )
    return {"mode": "review-deny", "review": review.to_json()}


def cmd_review_approve(data: dict[str, Any]) -> dict[str, Any]:
    review_id = data.get("review_id")
    if not review_id:
        raise SystemExit("review_id required")
    tools = _tools_from_request(data)
    review_preview = load_review(str(review_id), reviews_dir=data.get("reviews_dir"))
    if review_preview.tool_name not in tools:
        tools = dict(tools)
        tools[review_preview.tool_name] = ToolSpec(
            review_preview.tool_name,
            requires_confirmation=True,
            irreversible=True,
        )
        tools.setdefault("_gate.noop", ToolSpec("_gate.noop", "padding"))
    host = _host_from_request(data, tools)
    # Approver is a trusted human — ensure host can pass trust checks.
    host.source_trust = parse_source_trust("trusted")
    host.confirmation_given = True
    if host.tool_allowlist is not None:
        host.tool_allowlist = set(host.tool_allowlist) | {review_preview.tool_name}
    review, outcome = approve_review(
        str(review_id),
        tools,
        host,
        resolver=str(data.get("resolver") or "operator"),
        notes=str(data.get("notes") or ""),
        reviews_dir=data.get("reviews_dir"),
    )
    payload = outcome_to_dict(outcome)
    payload["mode"] = "review-approve"
    payload["review"] = review.to_json()
    payload["executor_log"] = host.executor.log  # type: ignore[attr-defined]
    return payload


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(prog="aos-gate", description=__doc__)
    parser.add_argument(
        "command",
        choices=(
            "evaluate",
            "dispatch",
            "record",
            "summarize",
            "review-create",
            "review-list",
            "review-approve",
            "review-deny",
        ),
        help="CLI mode (see module docstring)",
    )
    parser.add_argument(
        "--request",
        default="-",
        help="JSON request path (default: stdin)",
    )
    args = parser.parse_args(argv)
    data = _load_request(args.request)
    handlers = {
        "evaluate": cmd_evaluate,
        "dispatch": cmd_dispatch,
        "record": cmd_record,
        "summarize": cmd_summarize,
        "review-create": cmd_review_create,
        "review-list": cmd_review_list,
        "review-approve": cmd_review_approve,
        "review-deny": cmd_review_deny,
    }
    out = handlers[args.command](data)
    json.dump(out, sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
