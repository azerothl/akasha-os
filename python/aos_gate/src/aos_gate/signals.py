"""Derive Path A ``GateSignals`` from OS actor / policy context."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping

from akasha_model import GateSignals, ScoreLevel, ScoreQuestion
from akasha_model.primitives import score_result


@dataclass(frozen=True)
class GateContext:
    """Facts the OS already knows before asking the model gate."""

    has_required_capability: bool = True
    policy_allows: bool = True
    sufficient_context: bool = True
    confirmation_given: bool = False
    #: 0 = low, 1 = medium, 2 = high (matches demo risk Score).
    risk_level: int = 0
    confirmation_needed: float | None = None


def _risk_score(level: int):
    level = max(0, min(2, int(level)))
    mass = {0: 0.05, 1: 0.05, 2: 0.05}
    mass[level] = 0.90
    question = ScoreQuestion(
        "risk",
        "Rate the operational risk of executing the proposed tool call.",
        (ScoreLevel("low"), ScoreLevel("medium"), ScoreLevel("high")),
    )
    return score_result(question, mass)


def signals_from_context(ctx: GateContext) -> GateSignals:
    """Build explicit Path A signals (no neural scorer)."""
    authorized = 0.95 if ctx.policy_allows else 0.20
    sufficient = 0.92 if ctx.sufficient_context else 0.40
    capability = 0.94 if ctx.has_required_capability else 0.15
    confirm_needed = ctx.confirmation_needed
    if confirm_needed is None:
        confirm_needed = 0.05
    return GateSignals(
        authorized=authorized,
        sufficient_context=sufficient,
        capability_present=capability,
        confirmation_needed=confirm_needed,
        risk=_risk_score(ctx.risk_level),
        confirmation_given=ctx.confirmation_given,
    )


def context_from_mapping(data: Mapping[str, object] | None) -> GateContext:
    """Parse a JSON-ish gate context from the Rust bridge."""
    data = data or {}
    return GateContext(
        has_required_capability=bool(data.get("has_required_capability", True)),
        policy_allows=bool(data.get("policy_allows", True)),
        sufficient_context=bool(data.get("sufficient_context", True)),
        confirmation_given=bool(data.get("confirmation_given", False)),
        risk_level=int(data.get("risk_level", 0) or 0),
        confirmation_needed=(
            float(data["confirmation_needed"])
            if data.get("confirmation_needed") is not None
            else None
        ),
    )
