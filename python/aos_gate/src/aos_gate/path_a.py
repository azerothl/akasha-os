"""Path A: ``run_gated_call`` with an Akasha OS ``ToolHost``."""

from __future__ import annotations

from typing import Any, Mapping

from akasha_model import (
    GateSignals,
    ToolProposal,
    describe_outcome,
    run_gated_call,
)
from akasha_model.host import HostOutcome
from akasha_model.tool_calling import ToolSpec

from .host import AkashaOsToolHost
from .signals import GateContext, signals_from_context


def run_os_gated_call(
    tools: Mapping[str, ToolSpec],
    proposal: ToolProposal,
    host: AkashaOsToolHost,
    *,
    signals: GateSignals | None = None,
    context: GateContext | None = None,
) -> HostOutcome:
    """Authorize via akasha-model then dispatch to the OS host.

    Pass either explicit ``signals`` or a ``GateContext`` (OS-derived). When
    both are omitted, a permissive Path A default context is used.
    """
    if signals is None:
        signals = signals_from_context(context or GateContext())
    return run_gated_call(tools, proposal, signals, host)


def outcome_to_dict(outcome: HostOutcome) -> dict[str, Any]:
    """JSON-friendly HostOutcome for the Rust bridge / logs / UI."""
    plan = outcome.plan
    return {
        "action": outcome.action,
        "did_execute": outcome.did_execute,
        "host_reason": outcome.host_reason,
        "describe": describe_outcome(outcome),
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
        "result": outcome.result,
    }
