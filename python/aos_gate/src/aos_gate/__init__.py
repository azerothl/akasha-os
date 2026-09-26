"""Akasha OS host binding for the akasha-model tool authorization gate."""

from .catalog import tools_from_os_catalog, tools_from_tool_descs
from .host import AkashaOsToolHost, OsExecutor, RecordingExecutor
from .legacy import LEGACY_UNGATED_MARKER, legacy_ungated_message
from .path_a import outcome_to_dict, run_os_gated_call
from .signals import GateContext, signals_from_context

__all__ = [
    "AkashaOsToolHost",
    "GateContext",
    "LEGACY_UNGATED_MARKER",
    "OsExecutor",
    "RecordingExecutor",
    "legacy_ungated_message",
    "outcome_to_dict",
    "run_os_gated_call",
    "signals_from_context",
    "tools_from_os_catalog",
    "tools_from_tool_descs",
]

__version__ = "0.1.0"
