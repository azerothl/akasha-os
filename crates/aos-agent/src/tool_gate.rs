//! Bridge to the akasha-model tool gate (Path A) via `python -m aos_gate.cli`.
//!
//! Flow:
//! 1. System 2 proposes a tool + args (agent loop).
//! 2. This module asks `aos_gate` / `akasha_model.evaluate_gate` for a plan.
//! 3. OS mirrors `akasha_model.host.dispatch_plan`: only `ready` proceeds to
//!    existing `policy_deny` + invoke_* (no second ungated executor).
//!
//! When the Python gate is unavailable, callers log [`LEGACY_UNGATED_MARKER`]
//! and may continue on the historical path (flagged, not silent).

use crate::policy::policy_deny;
use crate::tools::ToolDesc;
use aos_proto::AgentPolicy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Audit / UI marker when a tool runs without the model gate.
pub const LEGACY_UNGATED_MARKER: &str = "LEGACY_UNGATED_TOOL_PATH";

const GATE_TIMEOUT: Duration = Duration::from_secs(30);

/// `HostOutcome.action` values from `akasha_model.host`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostAction {
    Executed,
    SkippedAbstain,
    SkippedBlocked,
    RejectedByHost,
}

impl HostAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::SkippedAbstain => "skipped_abstain",
            Self::SkippedBlocked => "skipped_blocked",
            Self::RejectedByHost => "rejected_by_host",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallPlanView {
    pub status: String,
    pub tool_name: Option<String>,
    pub arguments: Option<Value>,
    pub reason: String,
    pub executable: bool,
    #[serde(default)]
    pub choice_probability: f64,
    #[serde(default)]
    pub choice_confidence: f64,
    pub risk_score: Option<f64>,
}

/// OS-side outcome after gate evaluate + host dispatch (mirrors HostOutcome).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostOutcomeView {
    pub action: HostAction,
    pub host_reason: String,
    pub plan: ToolCallPlanView,
    pub describe: String,
}

impl HostOutcomeView {
    pub fn did_execute(&self) -> bool {
        matches!(self.action, HostAction::Executed)
    }

    /// One-line log for `AgentOutputEvent::Log` / room traces.
    pub fn log_line(&self) -> String {
        format!(
            "tool_gate: {} — {} (gate={})",
            self.action.as_str().to_ascii_uppercase(),
            self.host_reason,
            self.plan.status
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateMode {
    /// Try the Python gate; on failure fall back and flag legacy.
    Auto,
    /// Always require the gate (errors surface to the agent).
    Require,
    /// Skip the gate (explicit legacy; still flags).
    Off,
}

impl GateMode {
    pub fn from_env() -> Self {
        match std::env::var("AOS_MODEL_GATE")
            .unwrap_or_else(|_| "auto".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "0" | "off" | "false" | "no" | "legacy" => Self::Off,
            "1" | "on" | "true" | "require" | "required" => Self::Require,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug)]
pub enum GateBridgeError {
    Unavailable(String),
    Protocol(String),
    TimedOut,
}

impl std::fmt::Display for GateBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(s) => write!(f, "tool gate unavailable: {s}"),
            Self::Protocol(s) => write!(f, "tool gate protocol error: {s}"),
            Self::TimedOut => write!(f, "tool gate timed out"),
        }
    }
}

/// Context used to build Path A `GateSignals` in Python.
#[derive(Debug, Clone, Serialize)]
pub struct GateContextJson {
    pub has_required_capability: bool,
    pub policy_allows: bool,
    pub sufficient_context: bool,
    pub confirmation_given: bool,
    pub risk_level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_needed: Option<f64>,
}

pub fn legacy_ungated_message(entrypoint: &str, tool_name: &str) -> String {
    format!(
        "{LEGACY_UNGATED_MARKER}: {entrypoint} invoked {tool_name} \
         without akasha-model run_gated_call (gate unavailable or disabled)"
    )
}

fn tool_desc_json(tools: &[ToolDesc]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema,
                    "required_caps": t.required_caps,
                    "backend": format!("{:?}", t.backend),
                })
            })
            .collect(),
    )
}

fn resolve_python() -> Option<PathBuf> {
    static CACHED: OnceLock<Option<PathBuf>> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            if let Ok(p) = std::env::var("AOS_GATE_PYTHON") {
                let path = PathBuf::from(p);
                if path.is_file() {
                    return Some(path);
                }
            }
            // Dev layout: repo `.venv-aos-gate`
            for candidate in [
                Path::new(".venv-aos-gate/bin/python"),
                Path::new("/workspace/.venv-aos-gate/bin/python"),
            ] {
                if candidate.is_file() {
                    return Some(candidate.to_path_buf());
                }
            }
            which("python3").or_else(|| which("python"))
        })
        .clone()
}

fn which(bin: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let p = dir.join(bin);
            p.is_file().then_some(p)
        })
    })
}

fn run_cli(command: &str, request: &Value) -> Result<Value, GateBridgeError> {
    let python = resolve_python().ok_or_else(|| {
        GateBridgeError::Unavailable("no python interpreter for aos_gate".into())
    })?;
    let body = serde_json::to_string(request)
        .map_err(|e| GateBridgeError::Protocol(format!("serialize request: {e}")))?;
    let started = Instant::now();
    let mut child = Command::new(&python)
        .args(["-m", "aos_gate.cli", command, "--request", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| GateBridgeError::Unavailable(format!("spawn {python:?}: {e}")))?;
    if let Some(stdin) = child.stdin.take() {
        use std::io::Write;
        let mut stdin = stdin;
        stdin
            .write_all(body.as_bytes())
            .map_err(|e| GateBridgeError::Protocol(format!("write stdin: {e}")))?;
    }
    loop {
        if started.elapsed() > GATE_TIMEOUT {
            let _ = child.kill();
            return Err(GateBridgeError::TimedOut);
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(GateBridgeError::Protocol(format!("wait: {e}"))),
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| GateBridgeError::Protocol(format!("wait_with_output: {e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(GateBridgeError::Unavailable(format!(
            "aos_gate.cli exited {}: {err}",
            output.status
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with('{'))
        .ok_or_else(|| GateBridgeError::Protocol("no JSON object on stdout".into()))?;
    serde_json::from_str(line).map_err(|e| GateBridgeError::Protocol(format!("parse: {e}")))
}

/// Ask akasha-model (via aos_gate) for a ToolCallPlan (Path A evaluate).
pub fn evaluate_tool_plan(
    tools: &[ToolDesc],
    tool_name: &str,
    arguments: &Value,
    context: &GateContextJson,
) -> Result<ToolCallPlanView, GateBridgeError> {
    let request = json!({
        "catalog": tool_desc_json(tools),
        "proposal": {
            "tool_name": tool_name,
            "arguments": arguments,
        },
        "context": context,
    });
    let resp = run_cli("evaluate", &request)?;
    let plan = resp
        .get("plan")
        .cloned()
        .ok_or_else(|| GateBridgeError::Protocol("missing plan".into()))?;
    serde_json::from_value(plan)
        .map_err(|e| GateBridgeError::Protocol(format!("plan shape: {e}")))
}

fn host_caps_allow(required: &str, actor_caps: &[String]) -> bool {
    if actor_caps.iter().any(|c| c == required) {
        return true;
    }
    for held in actor_caps {
        if let Some(prefix) = held.strip_suffix(":**") {
            if required.starts_with(prefix) {
                return true;
            }
        } else if let Some(prefix) = held.strip_suffix(":*") {
            if required.starts_with(prefix) {
                return true;
            }
        }
    }
    false
}

/// Mirror of `akasha_model.host.dispatch_plan` for the Rust executor path.
///
/// Gate evaluation stays in akasha-model; this only routes the plan to OS
/// permission checks. Callers run the real `execute` only when this returns
/// [`HostAction::Executed`].
pub fn dispatch_plan_to_host(
    plan: &ToolCallPlanView,
    tool_name: &str,
    arguments: &Value,
    policy: &AgentPolicy,
    tools: &[ToolDesc],
    actor_caps: &[String],
) -> HostOutcomeView {
    if plan.status == "abstain" {
        return HostOutcomeView {
            action: HostAction::SkippedAbstain,
            host_reason: plan.reason.clone(),
            plan: plan.clone(),
            describe: format!(
                "SKIPPED_ABSTAIN: {} — {} (gate={})",
                plan.tool_name.as_deref().unwrap_or("—"),
                plan.reason,
                plan.status
            ),
        };
    }
    if plan.status != "ready" || !plan.executable {
        return HostOutcomeView {
            action: HostAction::SkippedBlocked,
            host_reason: plan.reason.clone(),
            plan: plan.clone(),
            describe: format!(
                "SKIPPED_BLOCKED: {} — {} (gate={})",
                plan.tool_name.as_deref().unwrap_or("—"),
                plan.reason,
                plan.status
            ),
        };
    }
    let gated_name = plan.tool_name.as_deref().unwrap_or(tool_name);
    if let Some(denial) = policy_deny(policy, gated_name) {
        return HostOutcomeView {
            action: HostAction::RejectedByHost,
            host_reason: denial.clone(),
            plan: plan.clone(),
            describe: format!(
                "REJECTED_BY_HOST: {gated_name} — {denial} (gate={})",
                plan.status
            ),
        };
    }
    if let Some(desc) = tools.iter().find(|t| t.name == gated_name) {
        for req in &desc.required_caps {
            if !host_caps_allow(req, actor_caps) {
                let reason = format!("host capability missing: {req}");
                return HostOutcomeView {
                    action: HostAction::RejectedByHost,
                    host_reason: reason.clone(),
                    plan: plan.clone(),
                    describe: format!(
                        "REJECTED_BY_HOST: {gated_name} — {reason} (gate={})",
                        plan.status
                    ),
                };
            }
        }
    }
    let _ = arguments;
    HostOutcomeView {
        action: HostAction::Executed,
        host_reason: "host executed after ready and permission check".into(),
        plan: plan.clone(),
        describe: format!(
            "EXECUTED: {gated_name} — host executed after ready and permission check (gate=ready)"
        ),
    }
}

/// Build Path A context from current OS policy / caps.
pub fn gate_context_for_tool(
    tool_name: &str,
    policy: &AgentPolicy,
    tools: &[ToolDesc],
    actor_caps: &[String],
    confirmation_given: bool,
) -> GateContextJson {
    let policy_allows = policy_deny(policy, tool_name).is_none();
    let has_required_capability = tools
        .iter()
        .find(|t| t.name == tool_name)
        .map(|t| {
            t.required_caps.is_empty()
                || t.required_caps
                    .iter()
                    .all(|req| host_caps_allow(req, actor_caps))
        })
        .unwrap_or(true);
    GateContextJson {
        has_required_capability,
        policy_allows,
        sufficient_context: true,
        confirmation_given,
        risk_level: 0,
        confirmation_needed: None,
    }
}

/// Result of attempting the gated path before OS execute.
#[derive(Debug)]
pub enum GateDecision {
    /// Gate ready + host permissions OK → caller must invoke the real executor.
    Proceed { outcome: HostOutcomeView },
    /// Do not execute; return `message` to the agent / UI.
    Refuse {
        outcome: HostOutcomeView,
        message: String,
    },
    /// Gate binary missing / disabled — caller may use legacy path (flagged).
    Legacy { warning: String },
}

/// Async wrapper: runs [`decide_gated_tool`] on a blocking pool.
pub async fn decide_gated_tool_async(
    tools: &[ToolDesc],
    tool_name: &str,
    arguments: &Value,
    policy: &AgentPolicy,
    actor_caps: &[String],
    entrypoint: &str,
    confirmation_given: bool,
) -> GateDecision {
    let tools = tools.to_vec();
    let tool_name = tool_name.to_string();
    let arguments = arguments.clone();
    let policy = policy.clone();
    let actor_caps = actor_caps.to_vec();
    let entrypoint = entrypoint.to_string();
    tokio::task::spawn_blocking(move || {
        decide_gated_tool(
            &tools,
            &tool_name,
            &arguments,
            &policy,
            &actor_caps,
            &entrypoint,
            confirmation_given,
        )
    })
    .await
    .unwrap_or_else(|e| GateDecision::Legacy {
        warning: format!(
            "{LEGACY_UNGATED_MARKER}: gate task join failed: {e}"
        ),
    })
}

/// Evaluate + dispatch. On Proceed, caller runs existing invoke_* only.
pub fn decide_gated_tool(
    tools: &[ToolDesc],
    tool_name: &str,
    arguments: &Value,
    policy: &AgentPolicy,
    actor_caps: &[String],
    entrypoint: &str,
    confirmation_given: bool,
) -> GateDecision {
    let mode = GateMode::from_env();
    if mode == GateMode::Off {
        return GateDecision::Legacy {
            warning: legacy_ungated_message(entrypoint, tool_name),
        };
    }
    let ctx = gate_context_for_tool(tool_name, policy, tools, actor_caps, confirmation_given);
    let plan = match evaluate_tool_plan(tools, tool_name, arguments, &ctx) {
        Ok(p) => p,
        Err(e) => {
            let warning = format!(
                "{}; {}",
                legacy_ungated_message(entrypoint, tool_name),
                e
            );
            if mode == GateMode::Require {
                let outcome = HostOutcomeView {
                    action: HostAction::SkippedBlocked,
                    host_reason: e.to_string(),
                    plan: ToolCallPlanView {
                        status: "blocked".into(),
                        tool_name: Some(tool_name.into()),
                        arguments: Some(arguments.clone()),
                        reason: e.to_string(),
                        executable: false,
                        choice_probability: 0.0,
                        choice_confidence: 0.0,
                        risk_score: None,
                    },
                    describe: format!("SKIPPED_BLOCKED: {tool_name} — {e}"),
                };
                return GateDecision::Refuse {
                    message: format!("outil refusé (gate requis): {e}"),
                    outcome,
                };
            }
            return GateDecision::Legacy { warning };
        }
    };
    let outcome = dispatch_plan_to_host(&plan, tool_name, arguments, policy, tools, actor_caps);
    match outcome.action {
        HostAction::Executed => GateDecision::Proceed { outcome },
        HostAction::SkippedAbstain | HostAction::SkippedBlocked | HostAction::RejectedByHost => {
            let message = format!(
                "outil refusé par le gate ({}): {}",
                outcome.action.as_str(),
                outcome.host_reason
            );
            GateDecision::Refuse { outcome, message }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{AgentNetPolicy, AgentPolicy};
    use crate::tools::{ToolBackend, ToolDesc};

    fn sample_tools() -> Vec<ToolDesc> {
        vec![
            ToolDesc {
                name: "fs.read".into(),
                description: "read".into(),
                input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
                backend: ToolBackend::Native,
                required_caps: vec!["fs.read:**".into()],
            },
            ToolDesc {
                name: "fs.write".into(),
                description: "write".into(),
                input_schema: json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}),
                backend: ToolBackend::Native,
                required_caps: vec!["fs.write:**".into()],
            },
        ]
    }

    #[test]
    fn dispatch_ready_with_caps_proceeds() {
        let tools = sample_tools();
        let plan = ToolCallPlanView {
            status: "ready".into(),
            tool_name: Some("fs.read".into()),
            arguments: Some(json!({"path": "/documents/a.md"})),
            reason: "all planner gates passed".into(),
            executable: true,
            choice_probability: 0.9,
            choice_confidence: 0.9,
            risk_score: Some(0.1),
        };
        let policy = AgentPolicy::default();
        let caps = vec!["fs.read:**".into()];
        let outcome = dispatch_plan_to_host(
            &plan,
            "fs.read",
            &json!({"path": "/documents/a.md"}),
            &policy,
            &tools,
            &caps,
        );
        assert_eq!(outcome.action, HostAction::Executed);
    }

    #[test]
    fn dispatch_ready_host_deny_policy() {
        let tools = sample_tools();
        let plan = ToolCallPlanView {
            status: "ready".into(),
            tool_name: Some("web.search".into()),
            arguments: Some(json!({"query": "x"})),
            reason: "ok".into(),
            executable: true,
            choice_probability: 0.9,
            choice_confidence: 0.9,
            risk_score: None,
        };
        let policy = AgentPolicy {
            net: AgentNetPolicy::Deny,
            ..Default::default()
        };
        let outcome = dispatch_plan_to_host(
            &plan,
            "web.search",
            &json!({"query": "x"}),
            &policy,
            &tools,
            &[],
        );
        assert_eq!(outcome.action, HostAction::RejectedByHost);
    }

    #[test]
    fn dispatch_abstain_skips() {
        let tools = sample_tools();
        let plan = ToolCallPlanView {
            status: "abstain".into(),
            tool_name: Some("fs.read".into()),
            arguments: None,
            reason: "low confidence".into(),
            executable: false,
            choice_probability: 0.4,
            choice_confidence: 0.4,
            risk_score: None,
        };
        let outcome = dispatch_plan_to_host(
            &plan,
            "fs.read",
            &json!({}),
            &AgentPolicy::default(),
            &tools,
            &["fs.read:**".into()],
        );
        assert_eq!(outcome.action, HostAction::SkippedAbstain);
    }
}
