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
from .outcomes_log import OutcomeLogConfig, record_host_outcome
from .signals import GateContext, signals_from_context


def run_os_gated_call(
    tools: Mapping[str, ToolSpec],
    proposal: ToolProposal,
    host: AkashaOsToolHost,
    *,
    signals: GateSignals | None = None,
    context: GateContext | None = None,
    success: bool | None = None,
    user_forced: bool = False,
    notes: str = "",
    review_id: str | None = None,
    record_outcome: bool = True,
    outcome_config: OutcomeLogConfig | None = None,
    open_hitl_on_escalate: bool = True,
    hitl_entrypoint: str = "",
) -> HostOutcome:
    """Authorize via akasha-model then dispatch to the OS host.

    Pass either explicit ``signals`` or a ``GateContext`` (OS-derived). When
    both are omitted, a permissive Path A default context is used.

    After the host returns, Path D appends a redacted outcomes JSONL row
    (unless ``record_outcome`` is False or logging is disabled). Escalate
    stand-ins may open a HITL review record (see ``aos_gate.hitl``).
    """
    ctx = context or GateContext()
    if signals is None:
        signals = signals_from_context(ctx)

    # Keep host trust / confirmation aligned with the gate context.
    host.source_trust = ctx.source_trust
    host.confirmation_given = ctx.confirmation_given
    if not host.tools:
        host.tools = dict(tools)

    outcome = run_gated_call(tools, proposal, signals, host)

    review = None
    if open_hitl_on_escalate:
        # Local import avoids import cycle at module load (hitl → path_a).
        from .hitl import create_review, should_open_review

        if should_open_review(outcome, ctx):
            review = create_review(
                outcome,
                context=ctx,
                entrypoint=hitl_entrypoint,
            )
            if not notes:
                notes = f"hitl_opened; review_id={review.review_id}"
            elif f"review_id={review.review_id}" not in notes:
                notes = f"{notes}; hitl_opened; review_id={review.review_id}"

    if record_outcome:
        record_host_outcome(
            outcome,
            success=success,
            user_forced=user_forced,
            notes=notes,
            signals=signals,
            source_trust=ctx.source_trust,
            review_id=review.review_id if review else review_id,
            config=outcome_config,
        )
    return outcome


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
