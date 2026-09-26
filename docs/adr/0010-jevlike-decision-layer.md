# ADR 0010: Jev-like semantic decision layer

**Language:** English | Français (follow-up)

> Date: 20/09/2026 · Status: **proposed**  
> Related: [azerothl/akasha-model](https://github.com/azerothl/akasha-model) · tip bus intents `d12b12e`  
> Supersedes: none (extends soft-routing paths; does not replace ADR 0002 placement)

## Context

Akasha Preview already separates **hard, fail-closed control** from **soft
routing heuristics**:

| Soft today | Where | Problem |
|---|---|---|
| Task complexity | `aos-agent` `task.assess` (`assess.rs`) | LLM JSON; unreadable → forced **complex** |
| Tool / act choice | agent loop (`agent_act`, `tools`, `tool_exec`) | Generative model proposes tools |
| Workload class | `aos-model` `classify_workload` (`subsystem.rs`) | Text heuristics → Chat / AgentTools / LongReasoning / Batch |
| Model local vs frontier | session/UI + catalogue | No central semantic router |

Hard gates must stay deterministic:

1. Capability checks on every intent (`required_caps`, bus `PermissionDenied`)
2. `PolicyEngine` (`aos-platform/policy.rs`) — YAML allow/deny/require_confirmation
3. `AgentPolicy` (`aos-agent/policy.rs`) — net/fs/tool allowlists, fail-closed
4. UI confirmations (device/USB/fs.host, Allow once/Always/Deny)
5. Host path denylist in `tool_exec`
6. `aos-placement` / `model.plan` — measured InferencePlan (not model-chosen VRAM)

Industry “System One” models (TypeSafe Jev, 2026-09) show a useful pattern:
**typed probabilistic decisions** (Choice / Score / Noul) with calibrated
confidence, without generating text. `azerothl/akasha-model` (Jevlike-based)
implements that pattern open-source, plus a deterministic `ToolCallPlanner`
that returns `ready | abstain | blocked` and **never executes**.

We need a first-class place in akasha-os for that layer: between agent/model
soft decisions and hard gates — not inside the microkernel, not instead of
GGUF chat models.

## Decision

### 1. Introduce `aos-decisiond` (userspace)

A dedicated bus service registered via `BusService::on`, same IPC frame as
other daemons:

```text
Intent { name, args, actor, actor_caps: cap://… } → Response | stream
```

It hosts the akasha-model scorer (byte encoder and/or frozen HF encoder) and
exposes **proposal-only** intents. It does not mint caps, run tools, load
GGUF weights for chat, or bypass PolicyEngine.

### 2. New intents (contract)

| Intent | Purpose |
|---|---|
| `decision.solve` | Generic: `state` + typed questions → distributions + confidences |
| `decision.plan_tool` | Tool Choice + Noul/Score gates → `ToolCallPlan` |
| `decision.assess_task` | Soft replacement path for `task.assess` |
| `decision.classify_workload` | Soft replacement path for `classify_workload` |

**Primitives** (aligned with akasha-model / Jev-like API):

- **Choice** — select among 2–255 named options + full probability vector + confidence
- **Score** — ordered 2–10 levels → expected scalar + distribution + confidence
- **Noul** — P(true) ∈ [0, 1] for a binary criterion

**`decision.plan_tool` statuses** (deterministic planner, not the neural net):

- `ready` — host may proceed to its own cap/policy/confirm checks
- `abstain` — confidence/probability below threshold, or model abstained
- `blocked` — missing tool, failed schema, failed auth/capability/context Noul, risk ceiling, or confirmation required

The planner **must not** invoke tools. Execution remains on existing intents
(`fs.*`, `web.*`, `net.*`, `module.invoke`, …) after hard gates.

### 3. Integration position

```text
User / agent loop
        │
        ▼
 Soft proposals     aos-decisiond  (Choice / Score / Noul / Planner)
        │
        ▼
 Hard gates         caps · PolicyEngine · AgentPolicy · UI confirm · path denylist
        │
        ▼
 Execution          tool_exec / model.infer / model.load / …
        │
        ▼
 Placement          aos-placement / model.plan   (unchanged, deterministic)
```

**May assist / replace (phased):**

- `task.assess` complexity labelling
- tool ranking before `tool_exec`
- `classify_workload` heuristic
- optional soft signals for confirmation UX (never skip `require_confirmation`)
- optional `needs_frontier` Noul as a **hint** to UI/session routing (not an automatic remote call)

**Must not replace:**

- bus cap grant/revoke / `PermissionDenied`
- PolicyEngine / AgentPolicy authority (TrustManager stays advisory)
- UI confirmation flows
- path denylist, canvas geometric validation
- GPU/CPU placement and thermal fallback chains

### 4. Rollout phases

| Phase | Behaviour |
|---|---|
| **A — Shadow** | Call decision intents in parallel; log proposal vs current decision; no user-visible change. Prefer hook via `agent.<id>.control` `ActDecision` where useful. |
| **B — Soft UI** | Surface proposals (complexity, tool, workload, frontier hint) in UI; human or existing LLM path still decides. |
| **C — Gated cutover** | Agent loop prefers `decision.plan_tool` / `decision.assess_task` when confidence ≥ policy thresholds; still fail-closed through caps/policy/confirm. |
| **D — Semantic IPC** | Optional: inter-agent messages evaluated (`allowed_action?`, `risk?`, `target_agent?`) before dispatch — still proposals only. |

Phase A is mandatory before B/C. Synthetic Akasha-OS training data is **not**
sufficient to enable hard cutover.

### 5. Policy thresholds (host-owned)

Thresholds live in host/agent policy config, not in model weights, e.g.:

- min Choice probability / confidence for `ready`
- Noul thresholds for `authorized`, `capability_present`, `sufficient_context`
- max risk Score before `blocked`
- mapping confidence bands → auto / verify-with-local-LLM / human / frontier

### 6. Resource isolation

`aos-decisiond` is separate from `aos-modeld` chat inference:

- decision path targets low latency (soft budget ≪ interactive generation)
- must not contend for the same exclusive GPU lock as large GGUF decode without an explicit Placement plan
- CPU / small-encoder path must work offline (Akasha offline-first)

### 7. Observability

Every `decision.*` response is auditable like other intents: actor, caps,
inputs hash, selected options, full distributions (or stored summary),
planner status/reason, latency. Shadow diffs (proposal vs legacy) are first-
class metrics for cutover gates.

## Consequences

### Positive

- Typed, non-generative control plane for agent routing and tool selection
- Clear boundary: probabilistic judgement vs deterministic enforcement
- Aligns with capability-based design and semantic IPC direction
- Opens a path to calibrated abstention instead of “LLM JSON or complex”

### Negative / risks

- New daemon and IPC surface to secure and version
- Early checkpoints may be weak on open-domain language (byte encoder); frozen HF encoder increases download/RAM
- Mis-calibrated confidence could over-automate if Phase C is rushed
- Duplicate logic during shadow (cost/latency) until legacy paths retire

### Mitigations

- Proposal-only API; hard gates unchanged
- Phase A metrics gate before any cutover
- Keep `task.assess` / heuristic workload as fallback on decisiond failure (fail-safe direction to be policy-defined; default conservative)
- No silent bypass of `require_confirmation` or remote-secret Policy rules

## Out of scope

- Replacing GGUF chat / coding / vision offerings in the Preview catalogue
- Training a commercial Jev-equivalent from scratch in this ADR
- Letting the decision model choose VRAM placement or revoke caps
- Cloud-only decision APIs without a local offline path
- Changing Preview mill / release process or VERSION bumps in this ADR
- Implementing `aos-decisiond` runtime code in the same change as this document (docs-only PR)

## References

- [azerothl/akasha-model](https://github.com/azerothl/akasha-model)
- OS host binding (Path A): [tool-gate-host.md](../tool-gate-host.md) · `python/aos_gate/`
- ADR 0002 Model Placement (`adr/0002-model-placement.md`)
- Tip bus intents inventory (platformd / modeld / agentd) at `d12b12e`
- TypeSafe: Introducing System One models and Jev (2026-09)
