"""Map MCP ``tools/list`` manifests to akasha-model ``ToolSpec`` catalogs.

MCP schemas rarely carry OS capabilities or confirmation metadata. This
module applies OS heuristics (caps, irreversible / confirmation) so
destructive or ambiguous tools do not default to silent ``ready``.
"""

from __future__ import annotations

from typing import Any, Iterable, Mapping

from akasha_model import ToolSpec

from .catalog import (
    _irreversible,
    _requires_confirmation,
    tools_from_tool_descs,
)

# MCP names that look destructive even without OS ToolDesc metadata.
_MCP_DESTRUCTIVE_HINTS = (
    "delete",
    "remove",
    "rm",
    "drop",
    "destroy",
    "kill",
    "revoke",
    "uninstall",
    "truncate",
    "overwrite",
    "write",
    "put",
    "update",
    "send",
    "execute",
    "run",
    "call",
)


def _as_mapping(value: Any) -> Mapping[str, Any]:
    if isinstance(value, Mapping):
        return value
    return {}


def mcp_tool_name(server: str | None, short_name: str) -> str:
    """Canonical OS name: ``mcp.<server>:<tool>`` when a server is known."""
    short = short_name.strip()
    if not server:
        return short
    if short.startswith(f"mcp.{server}:"):
        return short
    return f"mcp.{server}:{short}"


def _looks_destructive(name: str) -> bool:
    lower = name.lower()
    if _requires_confirmation(lower) or _irreversible(lower):
        return True
    return any(hint in lower for hint in _MCP_DESTRUCTIVE_HINTS)


def mcp_tool_to_desc(
    tool: Mapping[str, Any],
    *,
    server: str | None = None,
    required_caps: list[str] | None = None,
) -> dict[str, Any]:
    """Convert one MCP tools/list entry into an OS ToolDesc-shaped dict."""
    short = str(tool.get("name") or "").strip()
    if not short:
        raise ValueError("MCP tool missing name")
    name = mcp_tool_name(server, short)
    schema = tool.get("inputSchema")
    if schema is None:
        schema = tool.get("input_schema")
    if not isinstance(schema, Mapping):
        schema = {"type": "object"}
    caps = list(required_caps or [])
    if not caps and server:
        caps = [f"mcp.use:{server}"]
    # Fail closed: destructive / unknown write-ish tools need confirmation.
    force_confirm = _looks_destructive(short) or _looks_destructive(name)
    desc: dict[str, Any] = {
        "name": name,
        "description": str(tool.get("description") or ""),
        "input_schema": dict(schema),
        "required_caps": caps,
        "backend": "Mcp",
        "mcp_server": server,
        "mcp_short_name": short,
        # Extra hints consumed by tool_spec_from_desc via name heuristics;
        # also stored for docs / audits.
        "requires_confirmation": force_confirm,
        "irreversible": force_confirm,
    }
    return desc


def tools_from_mcp_list(
    tools: Iterable[Mapping[str, Any]],
    *,
    server: str | None = None,
    required_caps: list[str] | None = None,
) -> dict[str, ToolSpec]:
    """Build a gate-evaluable catalog from an MCP ``tools/list`` payload."""
    descs = [
        mcp_tool_to_desc(tool, server=server, required_caps=required_caps)
        for tool in tools
    ]
    catalog = tools_from_tool_descs(descs)
    # Reinforce confirmation flags on ToolSpec (name heuristics already apply;
    # re-apply for MCP short names that lost the hint after prefixing).
    for desc in descs:
        name = str(desc["name"])
        spec = catalog.get(name)
        if spec is None:
            continue
        if desc.get("requires_confirmation") or desc.get("irreversible"):
            catalog[name] = ToolSpec(
                name,
                description=spec.description,
                parameters=spec.parameters,
                required_capability=spec.required_capability,
                requires_confirmation=True,
                irreversible=True,
            )
    return catalog


def tools_from_mcp_manifest(
    manifest: Mapping[str, Any] | list[Any],
    *,
    server: str | None = None,
) -> dict[str, ToolSpec]:
    """Accept ``{\"tools\": [...]}``, ``{\"result\": {\"tools\": [...]}}``, or a list."""
    if isinstance(manifest, list):
        return tools_from_mcp_list(manifest, server=server)
    data = _as_mapping(manifest)
    if server is None and isinstance(data.get("server"), str):
        server = data["server"]
    tools = data.get("tools")
    if tools is None:
        result = data.get("result")
        if isinstance(result, Mapping):
            tools = result.get("tools")
    if not isinstance(tools, list):
        raise TypeError(
            "MCP manifest must be a tools list or object with tools/result.tools"
        )
    return tools_from_mcp_list(
        [_as_mapping(t) for t in tools],
        server=server,
        required_caps=(
            list(data["required_caps"])
            if isinstance(data.get("required_caps"), list)
            else None
        ),
    )


# Documented mapping gaps (kept in module for docs sync / tests).
MCP_MAPPING_GAPS = (
    "MCP tools/list has no first-class capability tokens — OS assigns "
    "mcp.use:<server> (or caller-supplied required_caps).",
    "MCP annotations (readOnlyHint / destructiveHint) are not yet wired; "
    "name heuristics mark write/delete/execute-like tools as confirmation+irreversible.",
    "Output schemas, resource templates, and sampling are out of scope for ToolSpec.",
    "Ambiguous destructive tools without confirmation metadata fail closed "
    "(requires_confirmation=True) rather than defaulting to ready.",
)
