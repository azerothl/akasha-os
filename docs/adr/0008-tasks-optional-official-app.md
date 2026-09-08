# ADR 0008: Tasks as an optional official module app

**Language:** English | Français (follow-up)

> Date: 08/09/2026 · Status: **accepted** (lot 0 — contract and map only)  
> Tracking: [issue #149](https://github.com/azerothl/akasha-os/issues/149)

## Context

Tasks is the user task list (`tasks.*` / `tasks.aospkg`): a shared checklist for humans
and agents. Business logic already lives in WASM (`modules/tasks`) and ships as
`share/modules/tasks.aospkg`. The host still treats Tasks as a special bundled
surface: forced boot resync, a native egui tab, static agent tool definitions, and
refused uninstall.

**External** means a **package** on the existing module runtime — not a cloud
service, not a process outside Akasha. Sources stay in the monorepo until a
separate delivery repo is justified.

**Do not confuse** `tasks.*` with platform services that stay in the host for this
programme:

| Namespace | Role | Stays platform |
|-----------|------|----------------|
| `tasks.*` | User task list (this ADR) | Becomes optional official app |
| `task.assess` | Agent bootstrap / goal assessment | Yes |
| `goal.*` | Agent goal loop completion | Yes |
| Agent plans / cognitive state | Internal worker state | Yes |
| `schedule.*` | Agent scheduling and persistence | Yes |

Lot 0 (this ADR) maps couplings and freezes the public contract. It does **not**
remove the native Tasks panel, stop boot resync, or extract Notes or Create (#150).

## Decision

1. **Tasks is preinstallable and uninstallable.** A standard profile may ship a
   verified local package; a minimal profile may omit it. **Preinstall is not
   undeletable** — the user can uninstall and that choice must persist.

2. **Protected is a host policy, not a package privilege.** Whether Tasks (or any
   official app) is protected from uninstall belongs in host configuration /
   migration policy. The package manifest must not grant itself undeletable status.

3. **Core updates must respect user choice.** A host upgrade must not reinstall
   Tasks after the user uninstalled it, and must not downgrade a newer standalone
   package the user installed.

4. **Compatibility contract is frozen** (see [tasks-contract.md](../tasks-contract.md)
   and `aos_proto::tasks_contract`). Tool ids, store path, and capability strings
   remain stable through extraction.

## Coupling map

Each row is a **special coupling on `main` at lot 0**. Destination names refer to
the lot sequence in issue #149 unless marked **keep**.

### Agent tools (`tasks.*`)

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-agent/src/tools.rs` | Static `ToolDesc` entries for `tasks.create/list/update/complete`; included in `default_agent_tools()` and permissive `select_tools` prefix match | **Lot 3** — build catalog from installed module `module.describe`; static entries removed after salon path shares discovery |
| `crates/aos-agent/src/tools.rs` | `select_tools` dedup keeps first name; static catalog wins over discovered tools today | **Lot 3** — manifest-installed tools become canonical |
| `crates/aos-agent/src/agent_act.rs` | `tasks.list` exempt from act confirmation; other `tasks.*` require confirm | **Keep** until lot 3 — policy may move to generic module-tool rules |
| `crates/aos-agent/src/tool_exec.rs` | `module.invoke` path for `tasks.*` | **Keep** — generic module invocation |
| `crates/aos-agent/src/bin/aos-agent-worker.rs` | `discover_module_tools()` via `module.list` + `module.describe` | **Lot 3** — extract shared discovery; worker already uses it but static tools still shadow |
| `crates/aos-agent/src/room_runtime.rs` | `assemble_room_member_tools()` merges `default_agent_tools()` (includes tasks) when spec tools empty; `select_tools(&tool_ids, &[])` **without** `discover_module_tools` | **Lot 3** — same shared discovery as worker |
| `crates/aos-agent/src/bin/aos-agentd.rs` | Scheduled / lightweight paths use `select_tools` only | **Lot 3** — align with shared discovery |
| `crates/aos-platform/src/skill_pass.rs` | Heuristic `infer_tools()` adds `tasks.list/create` when user text mentions "task" | **Keep** heuristic; **Lot 3** — skill `tasks` depends on module presence |

### UI navigation (`Tab::Tasks`)

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-ui-egui/src/main.rs` | `Tab::Tasks`, `ui_tasks()`, tab bar / overflow / keyboard routing | **Lot 2** — `Tab::Module("tasks")` via declarative UI; temporary redirect acceptable |
| `crates/aos-ui-egui/src/nav.rs` | `TabKind::Tasks` mapping | **Lot 2** — generic module tab |
| `crates/aos-ui-egui/src/i18n.rs` | `tab_tasks`, hints, tool labels, empty-state copy referencing `tasks.create` | **Lot 2** — labels move into package / declarative UI with FR/EN fallback |
| `crates/aos-ui-egui/src/runtime.rs` | Boot / smoke `invoke_tasks(...)` calls | **Keep** for Preview smoke until lot 5 — then module-generic smoke |
| `crates/aos-ui-egui/src/cmd.rs` | `Evt::TasksListed` | **Lot 2** — generic module binding refresh |
| `crates/aos-ui-egui/src/workspace_controller.rs` | `on_tasks_listed` | **Lot 2** — generic workspace events |
| `crates/aos-ui-egui/src/workspace_ui_state.rs` | `TasksPanelState` field | **Lot 5** — remove with native panel |

### Native panel (`tasks_panel.rs`)

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-ui-egui/src/tasks_panel.rs` | Native list UI, per-row complete/reopen, create form | **Lot 2** — declarative UI parity; **Lot 5** — delete file |
| `crates/aos-ui-egui/src/module_actions.rs` | `invoke_tasks()` hardcodes module name, caps, list-after-mutation | **Lot 2** — generic `invoke_module_tool` + binding invalidation |
| `website/docs/use.html`, `docs/FEATURES.md`, `docs/TESTER.md` | Document native Tasks tab | **Lot 5** — update to module-generic navigation |

### Bundled module policy (`BUNDLED_MODULES`)

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-proto/src/decl_ui.rs` | `BUNDLED_MODULES` includes `"tasks"`; `is_bundled_module()` | **Lot 1** — split *preinstalled*, *protected*, *native-tab*; Tasks leaves bundled list |
| `crates/aos-proto/src/decl_ui.rs` | `DECL_UI_SIDEBAR_EXCLUDE` hides bundled modules from generic sidebar | **Lot 2** — Tasks uses generic nav when installed |
| `crates/aos-platform/src/module_rt.rs` | `module.uninstall` refuses bundled names | **Lot 1** — uninstall allowed; host records user choice |
| `crates/aos-platform/src/bin/aos-platformd.rs` | Install/uninstall UI paths check `is_bundled_module` | **Lot 1** — policy-driven protection only |
| `crates/aos-ui-egui/src/ui_settings.rs` | Settings module list disables uninstall for bundled | **Lot 1** — show uninstall; optional host "protected" badge |

### Tasks skill

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `skills/tasks/SKILL.md` | Declares `tasks.*` tools (+ `goal.complete`) | **Lot 3** — skill availability tied to installed `tasks` module |
| `community/skills/morning-brief/SKILL.md` | Uses `tasks.list` read-only | **Lot 3** — graceful degradation when module absent |
| `crates/aos-ui-egui/src/agent_ui_state.rs` | Default selected tools include all four `tasks.*` | **Keep** default for standard profile; **Lot 3** — hide when module missing |
| `crates/aos-ui-egui/src/chat_delegate.rs` | Grants `tool.invoke:tasks` when skills/tools mention tasks | **Keep** cap grant pattern; module must be installed to invoke |

### Store path `/documents/tasks/tasks.json`

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `modules/tasks/src/lib.rs` | `TASKS_PATH`, JSON `TaskStore` schema, CRUD tools | **Keep** in package — contract frozen |
| `share/modules/tasks.aospkg/manifest.yaml` | `fs.read` / `fs.write` caps scoped to `/documents/tasks/**` | **Keep** — contract frozen |
| `share/modules/catalogue.yaml` | Signed catalogue entry with attested caps | **Keep** catalogue format; **Lot 5** — independent package versioning |
| `crates/aos-session/src/bootstrap.rs` | Tests reference tasks registry caps | **Keep** tests; update when policy changes |

**Load behaviour (known gap):** `Tasks::load()` maps **any** `fs_read` error to an empty
store. Required follow-up (not lot 0): missing file = first launch; permission denied /
parse errors must **not** silently wipe or masquerade as empty. See
`aos_proto::tasks_contract::LOAD_BEHAVIOUR_FOLLOWUP`.

### Boot resync

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-session/src/main.rs` | `sync_packaged_module(share/modules/tasks.aospkg → var/modules/tasks)` every boot; append registry caps if missing | **Lot 4** — generic preinstall/migration; **no resync after user uninstall** |
| `packaging/build-preview.sh` / `.ps1` / `modules/build-tasks.ps1` | Build WASM, copy `tasks.aospkg` into Preview tree | **Lot 5** — package builds independently; Preview consumes artefact |
| `README.md`, `website/docs/use.html` | "resyncs on boot" copy | **Lot 4** — document new lifecycle |

### Salon and worker discovery

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `crates/aos-agent/src/bin/aos-agent-worker.rs` | `discover_module_tools()` at worker start; merged with static `select_tools` | **Lot 3** — discovery only; drop static `tasks.*` |
| `crates/aos-agent/src/room_runtime.rs` | Salon turns use `select_tools` on merged ids **without** module discovery | **Lot 3** — shared `discover_module_tools` |
| `crates/aos-agent/src/tools.rs` | `module.list` / `module.describe` meta-tools in catalog | **Keep** — platform introspection |

### Package artefact (`tasks.aospkg`)

| Location | What it does | Destination / reason |
|----------|--------------|------------------------|
| `modules/tasks/` | WASM source (`module-tasks`) | **Keep** in monorepo |
| `share/modules/tasks.aospkg/` | Shipped package (manifest, wasm, stub UI) | **Keep**; UI stub invalid for generic renderer today |
| `share/modules/tasks.aospkg/ui/index.html` | `commands` list only — no `root` widget tree | **Lot 2** — valid `declarative_ui` document |
| `modules/build-tasks.ps1` | Canonical manifest generator (must stay in sync with share/) | **Lot 5** — single source; stop duplicate inline manifests in shell scripts |

## Frozen public contract

Canonical constants: `crates/aos-proto/src/tasks_contract.rs`  
Human-readable mirror: [docs/tasks-contract.md](../tasks-contract.md)  
Tests: `tasks_contract` unit tests + `aos-agent` catalog cross-check.

| Field | Frozen value |
|-------|----------------|
| Module name | `tasks` |
| Tool ids | `tasks.create`, `tasks.list`, `tasks.update`, `tasks.complete` |
| Store path | `/documents/tasks/tasks.json` |
| FS caps | `fs.read:/documents/tasks/**`, `fs.write:/documents/tasks/**` |
| Invoke cap | `tool.invoke:tasks` |

## Generic platform extensions (later PRs — not lot 0)

These are **not implemented** in lot 0. Later lots depend on them:

1. **Row action** — declarative per-row button invoking a module tool with row-derived
   args (complete/reopen without manual id entry). Needed for Tasks declarative parity
   (lot 2). See `crates/aos-proto/src/decl_ui.rs` widget vocabulary.

2. **Binding invalidation** — after `tasks.create` / `update` / `complete`, declarative
   UI must re-fetch `tasks.list` without Tasks-specific branches in
   `module_actions.rs` (lot 2).

3. **Transactional install** — stage package, validate WASM/manifest/UI/caps/
   `min_os_api`, then atomically activate; keep last good version on failure (lot 1).
   Pointer: `crates/aos-platform/src/module_rt.rs` `install` removes destination first.

4. **`min_os_api` enforced** — manifest field exists on `tasks.aospkg` but runtime does
   not reject incompatible packages today (lot 1).

5. **Shared module tool discovery** — one component for worker, salon, and scheduler
   paths (lot 3). Pointer: `discover_module_tools` in worker vs `select_tools(&[], &[])`
   in `room_runtime.rs`.

6. **Host install policy** — registry fields for `user_removed`, `preinstalled`,
   `protected_by_host`, package version > bundled (lots 1 and 4).

7. **Package integrity** — catalogue hash covers WASM only today; independent Tasks
   delivery needs manifest+UI+schemas fingerprint (lot 5).

## Consequences

- Lot 0 PR is documentation + contract tests only; runtime behaviour unchanged.
- Reviewers can trace every Tasks special case to a future lot or an explicit keep.
- Notes (#150) and Canvas reuse the same extensions once Tasks validates them.
- No version bump, no marketing tweet — cartography and contract gate only.

## Out of scope (issue #149)

Marketplace, new SDK, new package format, webview, scheduler extraction,
calendar/reminders/subtasks, moving sources out of the monorepo, extracting Notes or
Canvas in this PR, measured startup/binary-size claims.
