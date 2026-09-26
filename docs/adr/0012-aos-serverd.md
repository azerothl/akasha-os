# ADR 0012: Preview server lifecycle owner (`aos-serverd` vs `aos-session`)

**Language:** English | [Français](../fr/adr/0012-aos-serverd.md)

> Date: 26/09/2026 · Status: **proposed** (P21.0)  
> Tracking: [issue #403](https://github.com/azerothl/akasha-os/issues/403) · Preview **0.19.0 / P21**

## Context

Preview **0.18.0** ships `aos-session` as the interactive session supervisor:
it boots the process tree (`aos-busd` → `aos-capkd` → `aos-auditd` →
`aos-modeld` → `aos-platformd` → `aos-agentd`), launches `aos-ui-egui`, and
watchdogs **auditd / platformd / modeld** only. Closing the UI **stops** the
whole tree. Optional `aos-mcpd` / `aos-bridged` are not started by session.
`schedule.*` (E2) and E23 health exist inside a live session but are not an
OS-level always-on server.

Product need for **0.19.0**: run Akasha OS **without egui**, recover more
completely from crashes, accept **agent job intake** while headless, and
optionally install as a **user service** (systemd / launchd / Windows).

Sibling [Akasha](https://github.com/azerothl/akasha) already has a 24/7
assistant daemon. Preview anti-roadmap refuses merge, messaging channels, and
network-exposed admin without caps.

## Decision

### 1. Unique process-tree owner — new `aos-serverd` (option B)

Introduce a dedicated binary **`aos-serverd`** as the **lifecycle owner** of
the Preview daemon tree. Keep **`aos-session`** as interactive bootstrap + UI
client.

| Role | Owner |
|------|--------|
| Start / stop / ordered restart of busd…agentd | `aos-serverd` |
| Extended watchdogs (incl. **agentd**; busd/capkd with backoff) | `aos-serverd` |
| Local control plane (`status` / `restart` / `stop` / agent enqueue) | `aos-serverd` |
| Desktop onboarding, model download UX, egui launch | `aos-session` |
| Agent runtime, schedule, workers | `aos-agentd` (unchanged) |

**0.19 compatibility:** if no serverd is already up, `aos-session` may still
spawn the tree (legacy 0.18 behaviour) *or* start/attach to serverd. Document
a single owner: never double-supervise the same tree.

Reject for 0.19: folding server mode into `aos-session --headless` as the
long-term name (acceptable only as a temporary alias), and a generic
`aos-supervisord` abstraction.

### 2. Control plane — local, cap-gated, audited

Transport (Preview 0.19):

- Unix domain socket under `$AOS_HOME/var/run/aos-serverd.sock`, or
- Windows named pipe under the same `AOS_HOME` layout.

**No** bind to `0.0.0.0` / unauthenticated LAN in 0.19.

Every control command carries an actor identity + caps; restarts and job
enqueue are audited. `aos-serverd` **does not** mint capabilities.

Proposed local commands (exact intent names may land as `server.*` on the bus
or as socket framing — freeze in P21.3/P21.4):

| Command | Effect |
|---------|--------|
| `status` | Process tree + health summary |
| `restart` | Ordered stop/start (audited) |
| `stop` | Ordered shutdown |
| `enqueue-agent` / `server.job.*` | Intake → existing `aos-agentd` paths |

### 3. UI attach vs start

```text
[systemd/launchd/Win user service] → aos-serverd
                                       ├─ busd … agentd
                                       └─ control socket
[aos-session / egui]  ───────────────→ attach to live bus (preferred)
                                       or legacy-spawn if serverd absent
```

Closing egui **must not** stop the tree when serverd owns it. Session
“attach UI only” is a later lot (P21.6); MVP (P21.2) is headless tree up
without egui.

### 4. Agent intake vs `aos-agentd`

Intake is a **façade** into existing agentd APIs (`agent.start` / schedule /
spawn). Serverd does **not** reimplement the agent runtime, harness CLIs, or
tool execution. Caps fail-closed on enqueue.

### 5. Opt-in OS services

Packaging scripts for systemd **user**, launchd LaunchAgent, and Windows
logon task/service are **opt-in** (P21.5). Default desktop install remains
shortcut → `aos-session` without requiring a service.

## Non-goals (0.19.0)

- Sibling Akasha merge / Slack–Discord channels
- Multi-tenant / multi-user accounts
- Network control plane without auth
- Kubernetes / cloud orchestrator
- Replacing E23, Placement, or ADR 0010 (`aos-decisiond`)
- Spawning `aos-mcpd` / `aos-bridged` by default (opt-in P21.6)
- Binding host repos (`workspace.bind` is Track B / #247 in platformd)

## Consequences

- New crate/binary `aos-serverd` in the Preview zip (P21.2+).
- Factor shared spawn/stop/health out of `aos-session` (P21.1) before large
  moves.
- STATUS / FEATURES document P21 progress; VERSION **0.19.0** at release
  (P21.7).
- Docs EN+FR (INSTALL / TESTER headless) when MVP lands.

## Lots

| Lot | Content |
|-----|---------|
| **P21.0** | This ADR + STATUS “P21 in progress” + crate scaffold |
| **P21.1** | Shared spawn/stop/health lib; desktop session unchanged |
| **P21.2** | Headless tree without egui (**MVP**) |
| **P21.3** | Full watchdogs + local status/restart API (**MVP**) |
| **P21.4** | Agent job intake → agentd (**MVP**) |
| **P21.5** | Opt-in OS service scripts |
| **P21.6** | Opt-in mcpd/bridged; session attach-only |
| **P21.7** | Docs, website, VERSION 0.19.0 |

## Links

- Issue [#403](https://github.com/azerothl/akasha-os/issues/403)
- Companion Track B [#247](https://github.com/azerothl/akasha-os/issues/247)
- Code today: `crates/aos-session`, `crates/aos-agent`
- Related: [FEATURES.md](../FEATURES.md) (scheduler, MCP, isolated daemons)
