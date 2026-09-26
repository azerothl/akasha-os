"""Markers for execution paths that still skip the akasha-model gate."""

from __future__ import annotations

LEGACY_UNGATED_MARKER = "LEGACY_UNGATED_TOOL_PATH"

# Agent / room entry points that historically invoked tools without
# ``run_gated_call``. Gated cutover wraps them; when the Python gate binary is
# unavailable they log this marker instead of silently pretending to be gated.
LEGACY_ENTRYPOINTS = (
    "aos-agent-worker::execute_action",
    "aos-agent::tool_exec::execute_room_tool",
)


def legacy_ungated_message(entrypoint: str, tool_name: str) -> str:
    """One-line audit / UI log when a tool runs outside the model gate."""
    return (
        f"{LEGACY_UNGATED_MARKER}: {entrypoint} invoked {tool_name} "
        "without akasha-model run_gated_call (gate unavailable or disabled)"
    )
