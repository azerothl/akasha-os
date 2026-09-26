"""Path A host tests: ready→execute; blocked/abstain→no execute; host deny."""

from __future__ import annotations

from akasha_model import GateSignals, ToolProposal, ToolSpec, evaluate_gate
from akasha_model.host import dispatch_plan
from akasha_model.tool_calling import ToolCallPlanner

from aos_gate.catalog import tools_from_tool_descs
from aos_gate.host import AkashaOsToolHost, RecordingExecutor
from aos_gate.path_a import run_os_gated_call
from aos_gate.signals import GateContext, signals_from_context


def _catalog() -> dict[str, ToolSpec]:
    return tools_from_tool_descs(
        [
            {
                "name": "fs.read",
                "description": "Read a workspace file",
                "input_schema": {
                    "type": "object",
                    "required": ["path"],
                    "properties": {"path": {"type": "string"}},
                    "additionalProperties": False,
                },
                "required_caps": ["fs.read:**"],
                "backend": "Native",
            },
            {
                "name": "fs.write",
                "description": "Write a workspace file",
                "input_schema": {
                    "type": "object",
                    "required": ["path", "content"],
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"},
                    },
                    "additionalProperties": False,
                },
                "required_caps": ["fs.write:**"],
                "backend": "Native",
            },
        ]
    )


def _host(**kwargs) -> tuple[AkashaOsToolHost, RecordingExecutor]:
    executor = RecordingExecutor()
    tools = _catalog()
    host = AkashaOsToolHost(
        executor=executor,
        actor_caps={"fs.read:**", "fs.write:**"},
        tool_capabilities={
            name: spec.required_capability for name, spec in tools.items()
        },
        **kwargs,
    )
    return host, executor


def test_ready_executes_via_os_host():
    tools = _catalog()
    host, executor = _host()
    outcome = run_os_gated_call(
        tools,
        ToolProposal("fs.read", {"path": "/documents/notes.md"}),
        host,
        context=GateContext(has_required_capability=True, policy_allows=True),
    )
    assert outcome.action == "executed"
    assert outcome.did_execute
    assert executor.log == [
        {"tool": "fs.read", "arguments": {"path": "/documents/notes.md"}}
    ]


def test_blocked_and_abstain_do_not_execute():
    tools = _catalog()
    host, executor = _host()

    # High confirmation_needed noul without confirmation_given → blocked.
    blocked = run_os_gated_call(
        tools,
        ToolProposal("fs.write", {"path": "/documents/x.md", "content": "x"}),
        host,
        signals=GateSignals(
            authorized=0.95,
            sufficient_context=0.95,
            capability_present=0.95,
            confirmation_needed=0.9,
            confirmation_given=False,
        ),
    )
    assert blocked.action == "skipped_blocked"
    assert executor.log == []

    # Low choice probability → abstain (dispatch_plan path).
    abstain_plan = evaluate_gate(
        tools,
        ToolProposal(
            "fs.read",
            {"path": "/documents/notes.md"},
            choice_probabilities={"fs.read": 0.52, "fs.write": 0.48},
        ),
        signals_from_context(GateContext()),
        planner=ToolCallPlanner(min_choice_probability=0.60),
    )
    abstain = dispatch_plan(abstain_plan, host)
    assert abstain.action == "skipped_abstain"
    assert executor.log == []


def test_ready_but_host_deny_rejected_by_host():
    tools = _catalog()
    host, executor = _host(net_deny=False, fs_write=False)
    # Gate ready for fs.read, but allowlist + missing cap style deny via policy
    # on a different tool: force ready fs.read then deny with empty caps.
    host.actor_caps = set()
    outcome = run_os_gated_call(
        tools,
        ToolProposal("fs.read", {"path": "/documents/notes.md"}),
        host,
        context=GateContext(has_required_capability=True, policy_allows=True),
    )
    assert outcome.action == "rejected_by_host"
    assert "capability" in outcome.host_reason.lower() or "missing" in outcome.host_reason.lower()
    assert executor.log == []
    # execute must never have been called — RecordingExecutor stays empty.
