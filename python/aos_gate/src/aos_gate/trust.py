"""Source-trust / authority confusion controls (AIRGuard-style).

Untrusted context (retrieved docs, web, MCP tool output) may inform the
agent's *proposal*, but must not become the *authorization* for side
effects. OS ``check_permissions`` is the source of truth for the trust
graph; Path A signals may also reflect provenance.
"""

from __future__ import annotations

from enum import Enum
from typing import Iterable

from akasha_model.tool_calling import ToolSpec


class SourceTrust(str, Enum):
    """Trust tier of context that influenced a tool proposal."""

    TRUSTED = "trusted"  # user / session / operator
    MIXED = "mixed"  # trusted intent + untrusted retrieved evidence
    UNTRUSTED = "untrusted"  # only retrieved / web / MCP-derived evidence


def parse_source_trust(value: object | None) -> SourceTrust:
    if value is None or value == "":
        return SourceTrust.TRUSTED
    text = str(value).strip().lower()
    for tier in SourceTrust:
        if text == tier.value:
            return tier
    if text in {"user", "session", "operator", "human"}:
        return SourceTrust.TRUSTED
    if text in {"retrieved", "rag", "web", "mcp", "tool_output", "untrusted_only",
                "asr", "stt", "speech", "transcript", "whisper", "os_stt"}:
        return SourceTrust.UNTRUSTED
    if text in {"partial", "hybrid"}:
        return SourceTrust.MIXED
    return SourceTrust.TRUSTED


def tool_is_high_impact(
    tool_name: str,
    tools: dict[str, ToolSpec] | None = None,
    *,
    high_impact_names: Iterable[str] | None = None,
) -> bool:
    """True when the tool is irreversible / needs confirmation / listed."""
    if high_impact_names and tool_name in set(high_impact_names):
        return True
    if tools and tool_name in tools:
        spec = tools[tool_name]
        return bool(spec.irreversible or spec.requires_confirmation)
    lower = tool_name.lower()
    return (
        lower.endswith((".delete", ".rm", ".kill", ".revoke"))
        or "delete" in lower
        or tool_name in {"harness.run", "device.usb.write", "fs.write", "mix.apply", "mix.apply_batch"}
    )


def trust_allows_execution(
    trust: SourceTrust,
    tool_name: str,
    *,
    tools: dict[str, ToolSpec] | None = None,
    confirmation_given: bool = False,
    high_impact_names: Iterable[str] | None = None,
) -> tuple[bool, str]:
    """OS authority check: untrusted-only evidence cannot authorize high impact.

    - ``trusted``: allowed (subject to other host checks).
    - ``mixed``: high-impact requires explicit human confirmation.
    - ``untrusted``: never authorizes high-impact; low-impact read-ish tools
      still need other host checks but are not auto-blocked here.
    """
    high = tool_is_high_impact(
        tool_name, tools, high_impact_names=high_impact_names,
    )
    if trust is SourceTrust.TRUSTED:
        return True, "trusted proposal provenance"
    if trust is SourceTrust.MIXED:
        if high and not confirmation_given:
            return False, (
                "authority confusion: mixed/untrusted evidence cannot authorize "
                f"high-impact tool {tool_name} without trusted confirmation"
            )
        return True, "mixed provenance with confirmation or low impact"
    # UNTRUSTED
    if high:
        return False, (
            "authority confusion: untrusted-only context cannot authorize "
            f"high-impact tool {tool_name} (escalate for human review)"
        )
    return True, "untrusted provenance on non-high-impact tool"


def trust_signal_adjustments(trust: SourceTrust) -> dict[str, float]:
    """Optional Path A signal nudges (host enforcement remains authoritative)."""
    if trust is SourceTrust.TRUSTED:
        return {}
    if trust is SourceTrust.MIXED:
        return {
            "authorized_scale": 0.85,
            "confirmation_needed_floor": 0.55,
        }
    return {
        "authorized_scale": 0.35,
        "confirmation_needed_floor": 0.85,
        "sufficient_context_scale": 0.70,
    }
