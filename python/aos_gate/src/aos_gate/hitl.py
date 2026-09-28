"""HITL review loop for gate escalate (approve / deny before execution).

Upstream akasha-model may gain a first-class ``escalate`` status; until then
Akasha OS treats selected ``abstain`` / host-trust refusals as the stand-in
and persists a durable review record. Approve re-enters the gate with
explicit human confirmation; deny never executes. No silent auto-approve.
"""

from __future__ import annotations

import json
import os
import uuid
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Mapping

from akasha_model import ToolProposal
from akasha_model.host import HostOutcome
from akasha_model.tool_calling import ToolSpec

from .host import AkashaOsToolHost
from .outcomes_log import OutcomeLogConfig, record_host_outcome
from .path_a import run_os_gated_call
from .signals import GateContext
from .trust import SourceTrust

DEFAULT_REVIEWS_DIR = Path("var/gate/reviews")
ENV_REVIEWS_DIR = "AOS_GATE_REVIEWS_DIR"


def resolve_reviews_dir(explicit: str | Path | None = None) -> Path:
    if explicit is not None:
        return Path(explicit)
    env = os.environ.get(ENV_REVIEWS_DIR, "").strip()
    if env:
        return Path(env)
    return DEFAULT_REVIEWS_DIR


@dataclass
class GateReview:
    """Durable HITL item linked to a gated tool attempt."""

    review_id: str
    tool_name: str
    arguments: dict[str, Any]
    plan_status: str
    plan_reason: str
    host_action: str
    source_trust: str = SourceTrust.TRUSTED.value
    status: str = "pending"  # pending | approved | denied
    entrypoint: str = ""
    notes: str = ""
    created_at: str = field(
        default_factory=lambda: datetime.now(timezone.utc).isoformat(),
    )
    resolved_at: str | None = None
    resolver: str = ""
    resolution_notes: str = ""

    def to_json(self) -> dict[str, Any]:
        return asdict(self)

    @classmethod
    def from_json(cls, data: Mapping[str, Any]) -> GateReview:
        return cls(
            review_id=str(data["review_id"]),
            tool_name=str(data.get("tool_name") or ""),
            arguments=dict(data.get("arguments") or {}),
            plan_status=str(data.get("plan_status") or ""),
            plan_reason=str(data.get("plan_reason") or ""),
            host_action=str(data.get("host_action") or ""),
            source_trust=str(data.get("source_trust") or SourceTrust.TRUSTED.value),
            status=str(data.get("status") or "pending"),
            entrypoint=str(data.get("entrypoint") or ""),
            notes=str(data.get("notes") or ""),
            created_at=str(data.get("created_at") or ""),
            resolved_at=data.get("resolved_at"),
            resolver=str(data.get("resolver") or ""),
            resolution_notes=str(data.get("resolution_notes") or ""),
        )


def should_open_review(
    outcome: HostOutcome,
    context: GateContext | None = None,
) -> bool:
    """Escalate stand-in: open HITL when human review is warranted.

    Triggers:
    - explicit ``GateContext.needs_human_review``
    - ``abstain`` with elevated risk / confirmation (consequence over confidence)
    - host rejection citing authority confusion / untrusted provenance
    """
    ctx = context or GateContext()
    if ctx.needs_human_review:
        return True
    plan = outcome.plan
    if plan.status == "abstain":
        confirm = ctx.confirmation_needed
        if ctx.risk_level >= 1:
            return True
        if confirm is not None and float(confirm) >= 0.5:
            return True
    reason = (outcome.host_reason or "").lower()
    if outcome.action == "rejected_by_host" and (
        "authority confusion" in reason or "untrusted" in reason
    ):
        return True
    return False


def _review_path(reviews_dir: Path, review_id: str) -> Path:
    return reviews_dir / f"{review_id}.json"


def create_review(
    outcome: HostOutcome,
    *,
    context: GateContext | None = None,
    entrypoint: str = "",
    notes: str = "",
    reviews_dir: str | Path | None = None,
) -> GateReview:
    """Persist a pending review record (idempotent new id each call)."""
    ctx = context or GateContext()
    root = resolve_reviews_dir(reviews_dir)
    root.mkdir(parents=True, exist_ok=True)
    plan = outcome.plan
    review = GateReview(
        review_id=str(uuid.uuid4()),
        tool_name=plan.tool_name or "",
        arguments=dict(plan.arguments or {}),
        plan_status=plan.status,
        plan_reason=plan.reason,
        host_action=outcome.action,
        source_trust=ctx.source_trust.value
        if isinstance(ctx.source_trust, SourceTrust)
        else str(ctx.source_trust),
        entrypoint=entrypoint,
        notes=notes or "escalate stand-in (awaiting human approve/deny)",
    )
    path = _review_path(root, review.review_id)
    path.write_text(
        json.dumps(review.to_json(), ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return review


def load_review(
    review_id: str, *, reviews_dir: str | Path | None = None,
) -> GateReview:
    path = _review_path(resolve_reviews_dir(reviews_dir), review_id)
    if not path.is_file():
        raise FileNotFoundError(f"review not found: {review_id}")
    return GateReview.from_json(json.loads(path.read_text(encoding="utf-8")))


def list_reviews(
    *,
    status: str | None = "pending",
    reviews_dir: str | Path | None = None,
) -> list[GateReview]:
    root = resolve_reviews_dir(reviews_dir)
    if not root.is_dir():
        return []
    rows: list[GateReview] = []
    for path in sorted(root.glob("*.json")):
        review = GateReview.from_json(json.loads(path.read_text(encoding="utf-8")))
        if status is None or review.status == status:
            rows.append(review)
    return rows


def _save_review(review: GateReview, reviews_dir: Path) -> None:
    path = _review_path(reviews_dir, review.review_id)
    path.write_text(
        json.dumps(review.to_json(), ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )


def deny_review(
    review_id: str,
    *,
    resolver: str = "operator",
    notes: str = "",
    reviews_dir: str | Path | None = None,
    outcome_config: OutcomeLogConfig | None = None,
) -> GateReview:
    """Deny: never execute; mark resolved and log outcome notes."""
    root = resolve_reviews_dir(reviews_dir)
    review = load_review(review_id, reviews_dir=root)
    if review.status != "pending":
        raise ValueError(f"review {review_id} is not pending (status={review.status})")
    review.status = "denied"
    review.resolved_at = datetime.now(timezone.utc).isoformat()
    review.resolver = resolver
    review.resolution_notes = notes or "human denied"
    _save_review(review, root)
    # Synthetic host outcome for Path D audit (no execute).
    from akasha_model.tool_calling import ToolCallPlan

    plan = ToolCallPlan(
        "blocked",
        review.tool_name,
        review.arguments,
        f"HITL denied review {review.review_id}",
    )
    denied_outcome = HostOutcome(
        "skipped_blocked", plan, None, plan.reason,
    )
    record_host_outcome(
        denied_outcome,
        success=False,
        user_forced=False,
        notes=f"hitl_deny; review_id={review.review_id}; {review.resolution_notes}",
        source_trust=review.source_trust,
        review_id=review.review_id,
        config=outcome_config,
    )
    return review


def approve_review(
    review_id: str,
    tools: Mapping[str, ToolSpec],
    host: AkashaOsToolHost,
    *,
    resolver: str = "operator",
    notes: str = "",
    reviews_dir: str | Path | None = None,
    outcome_config: OutcomeLogConfig | None = None,
) -> tuple[GateReview, HostOutcome]:
    """Approve: re-enter gate with human confirmation; execute only if ready+perms.

    Sets ``user_forced=True`` on the outcome log. Does not skip
    ``evaluate_gate`` / ``check_permissions``. Does not auto-approve from
    model threshold suggestions.
    """
    root = resolve_reviews_dir(reviews_dir)
    review = load_review(review_id, reviews_dir=root)
    if review.status != "pending":
        raise ValueError(f"review {review_id} is not pending (status={review.status})")

    # Human authorization upgrades provenance for this re-entry only.
    context = GateContext(
        has_required_capability=True,
        policy_allows=True,
        sufficient_context=True,
        confirmation_given=True,
        risk_level=0,
        confirmation_needed=0.05,
        source_trust=SourceTrust.TRUSTED,
        needs_human_review=False,
    )
    outcome = run_os_gated_call(
        tools,
        ToolProposal(review.tool_name, review.arguments),
        host,
        context=context,
        user_forced=True,
        review_id=review.review_id,
        record_outcome=True,
        outcome_config=outcome_config,
        open_hitl_on_escalate=False,
    )
    review.status = "approved"
    review.resolved_at = datetime.now(timezone.utc).isoformat()
    review.resolver = resolver
    review.resolution_notes = notes or (
        f"human approved; host_action={outcome.action}"
    )
    _save_review(review, root)
    return review, outcome
