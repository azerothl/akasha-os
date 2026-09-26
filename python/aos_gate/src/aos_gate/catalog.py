"""Map Akasha OS tool catalogue entries to ``akasha_model.ToolSpec``."""

from __future__ import annotations

from typing import Any, Iterable, Mapping

from akasha_model import ToolSpec

# Tools that always require an explicit human confirmation noul / flag.
# Keep this narrow: marking routine writers (fs.write / files.generate) as
# irreversible would block the agent loop under Path A defaults.
_CONFIRM_SUFFIXES = (".delete", ".rm", ".kill", ".revoke")
_IRREVERSIBLE_NAMES = frozenset(
    {
        "harness.run",
        "device.usb.write",
    }
)


def _as_mapping(value: Any) -> Mapping[str, Any]:
    if isinstance(value, Mapping):
        return value
    return {}


def _required_capability(required_caps: Iterable[str] | None) -> str | None:
    caps = [c for c in (required_caps or []) if isinstance(c, str) and c.strip()]
    if not caps:
        return None
    # Gate ToolSpec carries a single capability token; keep the primary OS cap.
    return caps[0]


def _requires_confirmation(name: str) -> bool:
    lower = name.lower()
    return lower.endswith(_CONFIRM_SUFFIXES) or "delete" in lower


def _irreversible(name: str) -> bool:
    return name in _IRREVERSIBLE_NAMES or _requires_confirmation(name)


def tool_spec_from_desc(desc: Mapping[str, Any]) -> ToolSpec:
    """Convert one OS ``ToolDesc``-shaped dict into a ``ToolSpec``."""
    name = str(desc.get("name") or "").strip()
    if not name:
        raise ValueError("tool desc missing name")
    schema = desc.get("input_schema")
    parameters = schema if isinstance(schema, Mapping) else None
    required_caps = desc.get("required_caps")
    if not isinstance(required_caps, list):
        required_caps = []
    return ToolSpec(
        name,
        description=str(desc.get("description") or ""),
        parameters=parameters,
        required_capability=_required_capability(required_caps),
        requires_confirmation=_requires_confirmation(name),
        irreversible=_irreversible(name),
    )


def tools_from_tool_descs(descs: Iterable[Mapping[str, Any]]) -> dict[str, ToolSpec]:
    """Build the gate catalogue from a list of OS tool descriptors."""
    tools: dict[str, ToolSpec] = {}
    for desc in descs:
        spec = tool_spec_from_desc(_as_mapping(desc))
        tools[spec.name] = spec
    if len(tools) < 2:
        # evaluate_gate / choice_from_proposal require ≥ 2 options.
        tools.setdefault(
            "_gate.noop",
            ToolSpec("_gate.noop", "Internal gate padding option (never proposed)."),
        )
    return tools


def tools_from_os_catalog(catalog: Mapping[str, Any] | list[Any]) -> dict[str, ToolSpec]:
    """Accept either a bare list of ToolDesc dicts or ``{\"tools\": [...]}``."""
    if isinstance(catalog, list):
        return tools_from_tool_descs(catalog)
    tools = catalog.get("tools")
    if isinstance(tools, list):
        return tools_from_tool_descs(tools)
    raise TypeError("OS catalog must be a list of tool descs or {\"tools\": [...]}")
