"""Path D — production gate outcomes logging for Akasha OS.

Wraps ``akasha_model.outcomes`` so production / Preview traffic appends the
same JSONL shape. Never auto-mutates upstream ``DEFAULT_*`` thresholds.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping

from akasha_model import GateSignals
from akasha_model.host import HostOutcome
from akasha_model.outcomes import (
    GateOutcomeRecord,
    append_outcome,
    load_outcomes,
    record_from_host_outcome,
    suggest_threshold_updates,
    summarize_outcomes,
)

from .credentials import redact_arguments
from .trust import SourceTrust

DEFAULT_OUTCOMES_PATH = Path("var/gate/outcomes.jsonl")
ENV_OUTCOMES_PATH = "AOS_GATE_OUTCOMES_PATH"
ENV_OUTCOMES_MODE = "AOS_GATE_OUTCOMES"  # auto | on | off


def outcomes_enabled() -> bool:
    mode = os.environ.get(ENV_OUTCOMES_MODE, "auto").strip().lower()
    if mode in {"0", "off", "false", "no"}:
        return False
    if mode in {"1", "on", "true", "yes", "require", "required"}:
        return True
    # auto: enabled when a path is set or default var/ layout is writable intent
    return True


def resolve_outcomes_path(explicit: str | Path | None = None) -> Path:
    if explicit is not None:
        return Path(explicit)
    env = os.environ.get(ENV_OUTCOMES_PATH, "").strip()
    if env:
        return Path(env)
    return DEFAULT_OUTCOMES_PATH


@dataclass
class OutcomeLogConfig:
    """Where / whether to append GateOutcomeRecord rows."""

    path: Path = DEFAULT_OUTCOMES_PATH
    enabled: bool = True

    @classmethod
    def from_env(cls, explicit: str | Path | None = None) -> OutcomeLogConfig:
        return cls(path=resolve_outcomes_path(explicit), enabled=outcomes_enabled())


def _redacted_record(record: GateOutcomeRecord) -> GateOutcomeRecord:
    """Return a copy safe for persistence (no raw secrets in arguments)."""
    return GateOutcomeRecord(
        proposal_tool=record.proposal_tool,
        plan_status=record.plan_status,
        host_action=record.host_action,
        plan_reason=record.plan_reason,
        proposal_arguments=redact_arguments(record.proposal_arguments),
        success=record.success,
        user_forced=record.user_forced,
        notes=record.notes,
        signals=record.signals,
        recorded_at=record.recorded_at,
    )


def record_host_outcome(
    outcome: HostOutcome,
    *,
    success: bool | None = None,
    user_forced: bool = False,
    notes: str = "",
    signals: GateSignals | None = None,
    source_trust: SourceTrust | str | None = None,
    review_id: str | None = None,
    config: OutcomeLogConfig | None = None,
) -> GateOutcomeRecord | None:
    """Append one redacted outcome row. Returns the record, or None if disabled.

    Does **not** call ``suggest_threshold_updates`` or write planner defaults.
    """
    cfg = config or OutcomeLogConfig.from_env()
    if not cfg.enabled:
        return None

    extra_notes = notes
    if source_trust is not None:
        trust_value = (
            source_trust.value
            if isinstance(source_trust, SourceTrust)
            else str(source_trust)
        )
        tag = f"source_trust={trust_value}"
        extra_notes = f"{extra_notes}; {tag}".strip("; ").strip()
    if review_id:
        tag = f"review_id={review_id}"
        extra_notes = f"{extra_notes}; {tag}".strip("; ").strip()

    record = _redacted_record(
        record_from_host_outcome(
            outcome,
            success=success,
            user_forced=user_forced,
            notes=extra_notes,
            signals=signals,
        )
    )
    append_outcome(cfg.path, record)
    return record


def record_from_mapping(
    payload: Mapping[str, Any],
    *,
    config: OutcomeLogConfig | None = None,
) -> GateOutcomeRecord | None:
    """Append a row from a Rust/CLI JSON payload (HostOutcome-shaped)."""
    cfg = config or OutcomeLogConfig.from_env()
    if not cfg.enabled:
        return None
    plan = payload.get("plan") or {}
    record = _redacted_record(
        GateOutcomeRecord(
            proposal_tool=str(
                plan.get("tool_name")
                or payload.get("proposal_tool")
                or ""
            ),
            proposal_arguments=redact_arguments(
                plan.get("arguments")
                if isinstance(plan.get("arguments"), Mapping)
                else payload.get("proposal_arguments")
            ),
            plan_status=str(plan.get("status") or payload.get("plan_status") or ""),
            plan_reason=str(plan.get("reason") or payload.get("plan_reason") or ""),
            host_action=str(
                payload.get("action") or payload.get("host_action") or ""
            ),
            success=payload.get("success"),
            user_forced=bool(payload.get("user_forced", False)),
            notes=str(payload.get("notes") or ""),
            signals=payload.get("signals")
            if isinstance(payload.get("signals"), Mapping)
            else None,
        )
    )
    append_outcome(cfg.path, record)
    return record


def offline_summary(path: str | Path | None = None) -> dict[str, Any]:
    """Load JSONL and return summarize + suggest (suggestions never applied)."""
    target = resolve_outcomes_path(path)
    rows = load_outcomes(target)
    return {
        "path": str(target),
        "summary": summarize_outcomes(rows),
        "threshold_suggestions": suggest_threshold_updates(rows),
        "note": (
            "Suggestions are advisory only. Never auto-write akasha-model "
            "DEFAULT_* or silent OS policy changes."
        ),
    }


def dump_summary_json(path: str | Path | None = None) -> str:
    return json.dumps(offline_summary(path), ensure_ascii=False, indent=2)
