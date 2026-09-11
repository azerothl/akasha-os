# Module / DeclUI SDK — developer guide

**Language:** English | [Français](fr/module-sdk.md)

> Date: 11/09/2026 · Preview **0.17.0**

Guide for contributors and advanced testers packaging WASM modules with
declarative UI. For the first module without cargo, start at
[write-a-module.md](write-a-module.md). This document does not invent APIs —
it cites shipped paths, constants, and build scripts.

Related contracts:

- [rich-app-contract.md](rich-app-contract.md) — DeclUI v1/v2 gates and widget kinds
- [create-contract.md](create-contract.md) — reference rich app (`create`)
- [technical-specs.md](technical-specs.md) §0, §7 — architecture map
- [bridge/aos-proto-decl-ui.json](bridge/aos-proto-decl-ui.json) — exported JSON Schema

## What a module is

A Preview module is a signed `.aospkg` archive with:

```text
module.aospkg/
├── manifest.yaml      # name, version, hash, caps, tools, ui, min_os_api
├── module.wasm        # guest (script reuses ext-rt; rust compiles per module)
├── ui/                # declarative_ui document (JSON or legacy index.html)
├── schemas/           # optional tool input/output JSON schemas
└── signatures/        # catalogue signatures (maintainer builds)
```

Dual surface: **tools** for agents (`tool.invoke:<name>`) and **declarative_ui**
for humans (`module.ui` host tab). No webview on the Preview host.

## Versioning gates

Machine-checked in `crates/aos-proto/src/rich_app_contract.rs`:

| Gate | Constant / field | Current host |
|------|------------------|--------------|
| Host module API | `min_os_api` ≤ `OS_API_VERSION` (`1`) | `1` |
| DeclUI vocabulary | `ui.contract` | `1` (Notes/Tasks/script) or `2` (rich apps) |
| Async jobs facade | `services.jobs` | `1` when declared |
| Image generation facade | `services.media_image` | `1` (Create) |

Widget kind lists:

- v1: `decl_ui::WIDGET_KINDS` in `crates/aos-proto/src/decl_ui.rs`
- v2 additions: `UI_V2_ADDITIONAL_WIDGET_KINDS` in `rich_app_contract.rs`

Install is transactional: manifest, WASM hash, UI document, tool references,
and version gates are validated **before** activation. Failure discards staging
and keeps the previous package (`module_rt::install`).

## Path A — script module (no cargo)

Typical tester flow ([write-a-module.md](write-a-module.md)):

1. `module.scaffold` with `kind: script` → `var/modules/src/<name>/handlers.yaml`
2. Optional `ui` JSON in scaffold → copied into the package
3. `module.package` → `var/modules/packages/<name>.aospkg` (bundles prebuilt `ext-rt` WASM)
4. `module.install` → cap review (fail-closed); sidebar tab **Modules → &lt;name&gt;**

Name rules: 2–32 chars, `[a-z][a-z0-9-]*`, not a bundled name (`notes`, `tasks`,
`canvas`, `ext-rt`, `create`).

## Path B — Rust module (WASM SDK)

When script handlers are not enough:

1. `module.scaffold` with `kind: rust` under `var/modules/src/<name>/`
2. Depend on the guest SDK:

```toml
[dependencies]
aos-module-sdk = { path = "../../modules/sdk" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

3. Export handlers with `aos_module_sdk::export_module!(handle)` — guest talks to
   the host **only** via `host_call` (`fs.read`, `fs.write`, `fs.list`,
   `mem.episodic_write`, `mem.episodic_query`). No ambient WASI.
4. `module.compile` — critical capability; static refuse of `unsafe` and ambient
   `std::fs` / `net` / `process`; `CARGO_NET_OFFLINE=true`.
5. `module.package` + `module.install` as above.

SDK sources: [`modules/sdk`](../modules/sdk) (Apache-2.0, separate workspace).

## Maintainer build scripts

Official packages are rebuilt from `modules/` (not required for script modules):

| Package | Script | Output |
|---------|--------|--------|
| `notes` | `modules/build-notes.ps1` | `share/modules/notes.aospkg` |
| `tasks` | `modules/build-tasks.sh` / `.ps1` | `share/modules/tasks.aospkg` |
| `canvas` | `modules/build-canvas.sh` / `.ps1` | `share/modules/canvas.aospkg` |
| `create` | `modules/build-create.sh` | `share/modules/create.aospkg` |
| `ext-rt` | `modules/build-ext-rt.ps1` | precompiled runtime for script modules |
| `gallery-demo` | `modules/build-gallery-demo.sh` | demo of DeclUI v2 widgets |

Example (Create):

```bash
./modules/build-create.sh
```

Builds `modules/create/` → wasm32, validates UTF-8 JSON `ui/index.json`, writes
manifest with `ui.contract: 2`, copies to `share/modules/create.aospkg`, and may
refresh `share/modules/catalogue.yaml` hash.

## DeclUI document

- Entry: `ui/index.json` (rich apps) or legacy `ui/index.html` (v1 packages)
- Mode: `declarative_ui` in manifest
- Host intent: `module.ui` — platformd validates; host binds tool results and
  routes button/form submits through the same cap review as `module.invoke`
- JSON Schema: [`bridge/aos-proto-decl-ui.json`](bridge/aos-proto-decl-ui.json)

Rich apps add `state.local` / `state.document` slots, `bindings`, and `actions`
(see [rich-app-contract.md](rich-app-contract.md)). Composition widgets
(`layer_canvas`, `layer_list`, `undo_redo`) are generic — no Create-specific
host branches.

## Capabilities and catalogue

- Local signed catalogue: `share/modules/catalogue.yaml` + `catalogue.yaml.sig`
  (ed25519); Settings → Modules → Install still runs per-package cap review
- Opt-in community index: signed Git `community/catalogue.yaml` (cached offline)
- Uninstall: Settings → Installed modules; bundled apps can be removed — choice
  persists across boots (see [FEATURES.md](FEATURES.md))

Declared caps in `manifest.yaml` under `permissions.required_caps` must match
actual tool and service use. The host never grants a capability because a widget
names a tool.

## WASM ABI (guest-visible services)

Allowed via `host_call` (representative): `fs.read`, `fs.write`, `fs.list`,
`mem.episodic_write`, `mem.episodic_query`, `web.search`, `web.browse`,
`net.fetch`, `files.generate`, `mem.context`, `mem.user.*`, `mem.shared_*`,
`ext.load_handlers`.

**Prohibited inside guests:** `module.install`, `module.compile`, `secrets.get`,
`agent.*`, `trust.set`.

## Reference packages

| Name | `ui.contract` | Notes |
|------|---------------|-------|
| `notes` | 1 | hardcoded tab + WASM tools |
| `tasks` | 1 | Daily overflow slot when installed |
| `canvas` | 1 | session vector tools + declarative tab |
| `create` | 2 | rich workspace; [create-contract.md](create-contract.md) |
| `ext-rt` | — | script runtime, not a user-facing app |
| `gallery-demo` | 2 | DeclUI v2 widget gallery (maintainer) |

## Next steps

- Tester path: [TESTER.md](TESTER.md) §7 (module scenarios)
- Share a package: [community.md](community.md) (discussions / `community/modules/`)
- Target architecture: [technical-specs.md](technical-specs.md) §7
- ADR: [adr/0009-rich-module-app-contract.md](adr/0009-rich-module-app-contract.md)
