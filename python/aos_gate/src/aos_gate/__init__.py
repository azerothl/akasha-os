"""Akasha OS host binding for the akasha-model tool authorization gate."""

from .catalog import tools_from_os_catalog, tools_from_tool_descs
from .credentials import (
    FORBIDDEN_CREDENTIAL_KEYS,
    CredentialVault,
    find_credential_keys,
    inject_credentials,
    redact_arguments,
    reject_credential_arguments,
)
from .hitl import (
    GateReview,
    approve_review,
    create_review,
    deny_review,
    list_reviews,
    load_review,
    should_open_review,
)
from .host import AkashaOsToolHost, OsExecutor, RecordingExecutor
from .legacy import LEGACY_UNGATED_MARKER, legacy_ungated_message
from .mcp_catalog import (
    MCP_MAPPING_GAPS,
    tools_from_mcp_list,
    tools_from_mcp_manifest,
)
from .outcomes_log import (
    OutcomeLogConfig,
    offline_summary,
    record_from_mapping,
    record_host_outcome,
    resolve_outcomes_path,
)
from .path_a import outcome_to_dict, run_os_gated_call
from .signals import GateContext, signals_from_context
from .trust import SourceTrust, parse_source_trust, trust_allows_execution

__all__ = [
    "FORBIDDEN_CREDENTIAL_KEYS",
    "AkashaOsToolHost",
    "CredentialVault",
    "GateContext",
    "GateReview",
    "LEGACY_UNGATED_MARKER",
    "MCP_MAPPING_GAPS",
    "OsExecutor",
    "OutcomeLogConfig",
    "RecordingExecutor",
    "SourceTrust",
    "approve_review",
    "create_review",
    "deny_review",
    "find_credential_keys",
    "inject_credentials",
    "legacy_ungated_message",
    "list_reviews",
    "load_review",
    "offline_summary",
    "outcome_to_dict",
    "parse_source_trust",
    "record_from_mapping",
    "record_host_outcome",
    "redact_arguments",
    "reject_credential_arguments",
    "resolve_outcomes_path",
    "run_os_gated_call",
    "should_open_review",
    "signals_from_context",
    "tools_from_mcp_list",
    "tools_from_mcp_manifest",
    "tools_from_os_catalog",
    "tools_from_tool_descs",
    "trust_allows_execution",
]

__version__ = "0.2.0"
