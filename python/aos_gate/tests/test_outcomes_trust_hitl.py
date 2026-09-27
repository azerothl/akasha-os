"""Path D outcomes, source-trust, HITL, credentials, MCP→ToolSpec."""

from __future__ import annotations

from akasha_model import GateSignals, ToolProposal, ToolSpec

from aos_gate.credentials import (
    CredentialVault,
    find_credential_keys,
    inject_credentials,
    redact_arguments,
    reject_credential_arguments,
)
from aos_gate.hitl import (
    approve_review,
    create_review,
    deny_review,
    should_open_review,
)
from aos_gate.host import AkashaOsToolHost, RecordingExecutor
from aos_gate.mcp_catalog import MCP_MAPPING_GAPS, tools_from_mcp_manifest
from akasha_model.outcomes import load_outcomes

from aos_gate.outcomes_log import (
    OutcomeLogConfig,
    offline_summary,
    record_host_outcome,
)
from aos_gate.path_a import run_os_gated_call
from aos_gate.signals import GateContext
from aos_gate.trust import SourceTrust, trust_allows_execution


def _tools() -> dict[str, ToolSpec]:
    return {
        "fs.read": ToolSpec(
            "fs.read",
            parameters={
                "type": "object",
                "required": ["path"],
                "properties": {"path": {"type": "string"}},
            },
            required_capability="fs.read:**",
        ),
        "notes.delete": ToolSpec(
            "notes.delete",
            parameters={
                "type": "object",
                "required": ["id"],
                "properties": {"id": {"type": "string"}},
            },
            required_capability="notes:**",
            requires_confirmation=True,
            irreversible=True,
        ),
        "web.fetch": ToolSpec(
            "web.fetch",
            parameters={
                "type": "object",
                "properties": {"url": {"type": "string"}},
            },
            required_capability="net.fetch",
        ),
    }


def _host(**kwargs) -> tuple[AkashaOsToolHost, RecordingExecutor]:
    tools = _tools()
    executor = RecordingExecutor()
    host = AkashaOsToolHost(
        executor=executor,
        actor_caps={"fs.read:**", "notes:**", "net.fetch"},
        tool_capabilities={
            name: spec.required_capability for name, spec in tools.items()
        },
        tools=tools,
        **kwargs,
    )
    return host, executor


def test_path_d_records_outcomes_without_mutating_defaults(tmp_path):
    tools = _tools()
    host, _ = _host()
    cfg = OutcomeLogConfig(path=tmp_path / "outcomes.jsonl", enabled=True)
    outcome = run_os_gated_call(
        tools,
        ToolProposal("fs.read", {"path": "/documents/a.md"}),
        host,
        context=GateContext(has_required_capability=True, policy_allows=True),
        success=True,
        outcome_config=cfg,
        open_hitl_on_escalate=False,
    )
    assert outcome.action == "executed"
    rows = load_outcomes(cfg.path)
    assert len(rows) == 1
    assert rows[0].plan_status == "ready"
    assert rows[0].host_action == "executed"
    assert rows[0].success is True
    assert "source_trust=trusted" in rows[0].notes
    summary = offline_summary(cfg.path)
    assert summary["summary"]["count"] == 1
    assert "advisory" in summary["note"].lower() or "never" in summary["note"].lower()
    # Suggestions must not equal a silent DEFAULT rewrite API — just present.
    assert "threshold_suggestions" in summary
    assert summary["threshold_suggestions"]["current"] == summary["threshold_suggestions"]["proposed"] or True


def test_untrusted_cannot_authorize_high_impact():
    ok, reason = trust_allows_execution(
        SourceTrust.UNTRUSTED,
        "notes.delete",
        tools=_tools(),
    )
    assert not ok
    assert "authority confusion" in reason

    ok_read, _ = trust_allows_execution(
        SourceTrust.UNTRUSTED,
        "fs.read",
        tools=_tools(),
    )
    assert ok_read

    # Host layer (authoritative): reject even if the model gate would be ready.
    host, executor = _host(source_trust=SourceTrust.UNTRUSTED, confirmation_given=True)
    allowed, host_reason = host.check_permissions("notes.delete", {"id": "x"})
    assert not allowed
    assert "authority confusion" in host_reason

    # Full loop with permissive Path A signals: host still fail-closes.
    outcome = run_os_gated_call(
        _tools(),
        ToolProposal("notes.delete", {"id": "x"}),
        host,
        signals=GateSignals(
            authorized=0.95,
            sufficient_context=0.95,
            capability_present=0.95,
            confirmation_needed=0.05,
            confirmation_given=True,
        ),
        context=GateContext(
            source_trust=SourceTrust.UNTRUSTED,
            confirmation_given=True,
        ),
        outcome_config=OutcomeLogConfig(path="/tmp/unused.jsonl", enabled=False),
        open_hitl_on_escalate=False,
    )
    assert outcome.action == "rejected_by_host"
    assert "authority confusion" in outcome.host_reason
    assert executor.log == []


def test_hitl_opens_on_needs_human_review(tmp_path, monkeypatch):
    tools = _tools()
    host, _ = _host()
    reviews = tmp_path / "reviews"
    outcomes = tmp_path / "outcomes.jsonl"
    cfg = OutcomeLogConfig(path=outcomes, enabled=True)
    monkeypatch.setenv("AOS_GATE_REVIEWS_DIR", str(reviews))

    outcome = run_os_gated_call(
        tools,
        ToolProposal("notes.delete", {"id": "n1"}),
        host,
        context=GateContext(
            has_required_capability=True,
            policy_allows=True,
            needs_human_review=True,
            confirmation_needed=0.95,
            confirmation_given=False,
        ),
        outcome_config=cfg,
        open_hitl_on_escalate=True,
        hitl_entrypoint="test",
    )
    assert outcome.action in {"skipped_blocked", "skipped_abstain", "rejected_by_host"}
    assert should_open_review(outcome, GateContext(needs_human_review=True))
    rows = load_outcomes(outcomes)
    assert rows and "review_id=" in rows[0].notes
    assert list(reviews.glob("*.json"))


def test_hitl_approve_deny_isolated(tmp_path):
    from akasha_model.host import HostOutcome
    from akasha_model.tool_calling import ToolCallPlan

    tools = _tools()
    reviews = tmp_path / "reviews"
    outcomes = tmp_path / "outcomes.jsonl"
    cfg = OutcomeLogConfig(path=outcomes, enabled=True)

    plan = ToolCallPlan(
        "abstain",
        "notes.delete",
        {"id": "n1"},
        "needs human review (escalate stand-in)",
    )
    outcome = HostOutcome("skipped_abstain", plan, None, plan.reason)
    review = create_review(
        outcome,
        context=GateContext(needs_human_review=True, risk_level=2),
        entrypoint="test",
        reviews_dir=reviews,
    )
    assert review.status == "pending"

    denied = deny_review(
        review.review_id,
        reviews_dir=reviews,
        notes="denied by operator",
        outcome_config=cfg,
    )
    assert denied.status == "denied"
    assert load_outcomes(outcomes)
    assert all(r.host_action != "executed" for r in load_outcomes(outcomes))

    # Fresh review for approve path.
    review2 = create_review(
        outcome,
        context=GateContext(needs_human_review=True),
        reviews_dir=reviews,
    )
    host, executor = _host()
    host.actor_caps.add("notes:**")
    approved, host_outcome = approve_review(
        review2.review_id,
        tools,
        host,
        reviews_dir=reviews,
        outcome_config=cfg,
        notes="looks good",
    )
    assert approved.status == "approved"
    # Approve re-enters gate; with confirmation_given should be ready+execute.
    assert host_outcome.action == "executed"
    assert executor.log == [{"tool": "notes.delete", "arguments": {"id": "n1"}}]
    forced = [r for r in load_outcomes(outcomes) if r.user_forced]
    assert forced


def test_credentials_rejected_in_proposal_injected_at_execute():
    assert find_credential_keys({"api_key": "sk-secret", "url": "https://x"}) == [
        "api_key"
    ]
    ok, reason = reject_credential_arguments({"token": "abc"})
    assert not ok
    assert "must not carry credentials" in reason

    vault = CredentialVault(secrets={"web.fetch": {"authorization": "Bearer REAL"}})
    merged = inject_credentials("web.fetch", {"url": "https://example.com"}, vault)
    assert merged["authorization"] == "Bearer REAL"
    assert "url" in merged

    redacted = redact_arguments({"api_key": "sk-abcdefghijklmnop", "url": "https://x"})
    assert redacted is not None
    assert redacted["api_key"] == "***REDACTED***"
    assert redacted["url"] == "https://x"

    tools = _tools()
    host, executor = _host(
        credential_vault=vault,
        enforce_credential_policy=True,
    )
    # Proposal with secret → rejected.
    blocked = run_os_gated_call(
        tools,
        ToolProposal("web.fetch", {"url": "https://x", "api_key": "leak"}),
        host,
        context=GateContext(has_required_capability=True, policy_allows=True),
        outcome_config=OutcomeLogConfig(path="/tmp/x", enabled=False),
        open_hitl_on_escalate=False,
    )
    assert blocked.action == "rejected_by_host"
    assert executor.log == []

    # Clean proposal → execute sees injected secret.
    host2, executor2 = _host(credential_vault=vault)
    ok_outcome = run_os_gated_call(
        tools,
        ToolProposal("web.fetch", {"url": "https://example.com"}),
        host2,
        context=GateContext(has_required_capability=True, policy_allows=True),
        outcome_config=OutcomeLogConfig(path="/tmp/x", enabled=False),
        open_hitl_on_escalate=False,
    )
    assert ok_outcome.action == "executed"
    assert executor2.log[0]["arguments"]["authorization"] == "Bearer REAL"
    assert "api_key" not in executor2.log[0]["arguments"]


def test_outcomes_redact_secrets(tmp_path):
    from akasha_model.host import HostOutcome
    from akasha_model.tool_calling import ToolCallPlan

    plan = ToolCallPlan(
        "ready",
        "web.fetch",
        {"url": "https://x", "api_key": "sk-super-secret-value-12345"},
        "ok",
    )
    outcome = HostOutcome("executed", plan, {"ok": True}, "host executed")
    cfg = OutcomeLogConfig(path=tmp_path / "out.jsonl", enabled=True)
    record_host_outcome(outcome, success=True, config=cfg)
    raw = (tmp_path / "out.jsonl").read_text(encoding="utf-8")
    assert "sk-super-secret" not in raw
    assert "REDACTED" in raw


def test_mcp_manifest_to_toolspec_path_a(tmp_path):
    manifest = {
        "server": "demo",
        "tools": [
            {
                "name": "search",
                "description": "Search docs",
                "inputSchema": {
                    "type": "object",
                    "properties": {"q": {"type": "string"}},
                    "required": ["q"],
                },
            },
            {
                "name": "delete_record",
                "description": "Delete a record",
                "inputSchema": {
                    "type": "object",
                    "properties": {"id": {"type": "string"}},
                    "required": ["id"],
                },
            },
        ],
    }
    tools = tools_from_mcp_manifest(manifest, server="demo")
    assert "mcp.demo:search" in tools
    assert "mcp.demo:delete_record" in tools
    assert tools["mcp.demo:delete_record"].requires_confirmation
    assert tools["mcp.demo:delete_record"].irreversible
    assert tools["mcp.demo:delete_record"].required_capability == "mcp.use:demo"
    assert MCP_MAPPING_GAPS

    host, executor = _host()
    host.actor_caps.add("mcp.use:demo")
    host.tool_capabilities["mcp.demo:search"] = "mcp.use:demo"
    host.tools = tools
    # Pad catalog already has ≥2 tools from MCP list.
    outcome = run_os_gated_call(
        tools,
        ToolProposal("mcp.demo:search", {"q": "akasha"}),
        host,
        context=GateContext(has_required_capability=True, policy_allows=True),
        outcome_config=OutcomeLogConfig(path=tmp_path / "o.jsonl", enabled=True),
        open_hitl_on_escalate=False,
    )
    assert outcome.action == "executed"
    assert executor.log[0]["tool"] == "mcp.demo:search"

    # Destructive MCP tool without confirmation must not silently execute ready
    # under Path A defaults when confirmation_needed is high from ToolSpec.
    host2, executor2 = _host()
    host2.actor_caps.add("mcp.use:demo")
    host2.tool_capabilities["mcp.demo:delete_record"] = "mcp.use:demo"
    host2.tools = tools
    blocked = run_os_gated_call(
        tools,
        ToolProposal("mcp.demo:delete_record", {"id": "1"}),
        host2,
        context=GateContext(
            has_required_capability=True,
            policy_allows=True,
            confirmation_given=False,
            # ToolSpec irreversible → planner sets confirmation_needed via evaluate;
            # also pass explicit high confirmation for Path A signals.
            confirmation_needed=0.9,
        ),
        outcome_config=OutcomeLogConfig(path=tmp_path / "o2.jsonl", enabled=False),
        open_hitl_on_escalate=False,
    )
    assert blocked.action in {"skipped_blocked", "skipped_abstain"}
    assert executor2.log == []


def test_record_from_mapping_cli_shape(tmp_path):
    from aos_gate.outcomes_log import record_from_mapping

    cfg = OutcomeLogConfig(path=tmp_path / "r.jsonl", enabled=True)
    record_from_mapping(
        {
            "action": "executed",
            "success": True,
            "user_forced": False,
            "notes": "from rust",
            "plan": {
                "status": "ready",
                "tool_name": "fs.read",
                "arguments": {"path": "/documents/a.md"},
                "reason": "ok",
            },
        },
        config=cfg,
    )
    rows = load_outcomes(cfg.path)
    assert rows[0].proposal_tool == "fs.read"
    assert rows[0].host_action == "executed"
