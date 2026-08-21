# Sibling bridge — Akasha OS ↔ Akasha assistant

**Language:** English | [Français](fr/sibling-bridge.md)

> Date: 20/08/2026 · Preview **0.10.0**  
> Status: minimal live adapter (`aos-bridged`) + JSON Schema export  
> Related: [competitive-landscape.md](competitive-landscape.md), [evolution-roadmap.md](evolution-roadmap.md) E8, [docs/bridge/](bridge/)

## Principle

Do **not** merge [azerothl/akasha](https://github.com/azerothl/akasha) (personal
assistant daemon) and **akasha-os** into one binary. Align schemas so work is
not duplicated blindly; optional “assistant as module” remains a later option.

## OS contracts (source of truth in this repo)

| Area | Live schema | Notes |
|------|-------------|-------|
| Intents / IPC | [`crates/aos-proto`](../crates/aos-proto/src/lib.rs) + CBOR bus | Typed intents; `Intent.from` is caller identity |
| JSON Schema export | [`docs/bridge/`](bridge/) | Draft-07 JSON view of `mem.*`, `secrets.*`, and E15 `declarative_ui` |
| Memory | `mem.*` + E6 relations + E14 `mem.extract` | Flat episodic JSONL + `relations.jsonl`; chat→LT extract opt-in |
| Secrets | `secrets.get` / `set` / `list` | Encrypted vault; master key in OS keyring (file 0600 fallback); raw values only for services (`platformd`, `modeld`, `agentd`) |
| WASM ABI | [`module_rt`](../crates/aos-platform/src/module_rt.rs) + [`modules/sdk`](../modules/sdk) | Dual-surface (tools + UI); `host_call` only; `secrets.get` prohibited in guest |

## Mapping (conceptual)

| Sibling (assistant) | Akasha OS | Align? |
|---------------------|-----------|--------|
| Typed LT memory graph | E6 `MemRelationKind` | Yes — names aligned; storage differs |
| Vault / keyring | E7 `vault.enc` + OS keyring | Yes — never raw keys to agents |
| Wasmtime plugins (tools-only) | Dual-surface `.aospkg` | Partial — different contract; do not unify ABIs yet |
| Chat channels / 24/7 gateway | Out of OS core | Leave to sibling |
| Cron / calendar | OS `schedule.*` (E2) | Complementary |
| Auto fact extraction | E14 `mem.extract` | Conceptual peer; OS path is bus-native |

## HTTP JSON ↔ CBOR intent mapping

Preview **0.10.0** ships an optional separate binary `aos-bridged`
(`crates/aos-bridge`). It is **not** started by `aos-session`. Bind is always
`127.0.0.1` (`AOS_BRIDGE_PORT`, default `24710`). Bus: `AOS_BUS_ADDR`
(default `127.0.0.1:24701`). Smoke: `.\demo\smoke-bridge.ps1`.

- Base URL: `http://127.0.0.1:<bridge-port>/v1`
- Request body: JSON matching [`docs/bridge/`](bridge/)
- Response body: JSON of the same `aos-proto` type the bus would CBOR-encode
- Identity: header `X-Aos-From` → bus `Intent.from` (default `service:bridge`;
  do **not** trust a JSON `actor` field when it diverges)
- Errors: HTTP 403 if `Intent.from` is an agent and the intent is `secrets.get`

Live in 0.10 (minimal): `GET /health`, `POST /mem/stats`, `/mem/working_set`,
`/mem/working_get`, `/mem/episodic_write`, `/mem/episodic_query`,
`/mem/context`, `/mem/user/remember`, `/mem/user/recall`, `/secrets/list`,
`/secrets/get`, `/secrets/set`. Remaining mem.* routes stay contract-only
until a later increment.

| HTTP | Intent | JSON body schema (`$defs`) | Notes |
|------|--------|----------------------------|-------|
| `GET /health` | — | — | Process liveness (no bus) |
| `POST /mem/working_set` | `mem.working_set` | `MemWorkingRequest` | |
| `POST /mem/working_get` | `mem.working_get` | `MemWorkingRequest` | `messages` ignored |
| `POST /mem/episodic_write` | `mem.episodic_write` | `MemEpisodicWriteRequest` | |
| `POST /mem/episodic_query` | `mem.episodic_query` | `MemEpisodicQueryRequest` | |
| `POST /mem/episodic_delete` | `mem.episodic_delete` | `MemEpisodicDeleteRequest` | contract only |
| `POST /mem/stats` | `mem.stats` | (empty object) | Response: `MemStats` |
| `POST /mem/shared_read` | `mem.shared_read` | `MemSharedReadRequest` | contract only |
| `POST /mem/shared_write` | `mem.shared_write` | `MemSharedWriteRequest` | contract only |
| `POST /mem/user/remember` | `mem.user.remember` | `MemUserRememberRequest` | Response: `MemRememberResponse` |
| `POST /mem/user/recall` | `mem.user.recall` | `MemUserRecallRequest` | |
| `POST /mem/context` | `mem.context` | `MemContextRequest` | Response: `MemContextResponse` |
| `POST /mem/extract` | `mem.extract` | `MemExtractRequest` | contract only |
| `POST /mem/relate` | `mem.relate` | `MemRelateRequest` | contract only |
| `POST /mem/unrelate` | `mem.unrelate` | `MemUnrelateRequest` | contract only |
| `POST /mem/neighbors` | `mem.neighbors` | `MemNeighborsRequest` | contract only |
| `POST /mem/list` | `mem.list` | `MemListRequest` | contract only |
| `POST /mem/update` | `mem.update` | `MemUpdateRequest` | contract only |
| `POST /secrets/get` | `secrets.get` | `SecretGetRequest` | Services only; 403 for agents |
| `POST /secrets/set` | `secrets.set` | `SecretSetRequest` | Empty `value` deletes |
| `POST /secrets/list` | `secrets.list` | `SecretListRequest` | Names only; Response: `SecretListResponse` |

Wire note: OS daemons speak **CBOR**. The adapter transcodes JSON ↔ CBOR
1:1 using the same serde field names. Do not invent a second payload dialect.

## Non-goals

- Shared process or single installer
- Folding HTTP into `aos-session` / `aos-platformd`
- Reimplementing OpenClaw-class channels inside the OS
- Copy-paste of sibling source into this tree
- “Assistant as module” façade (post-contract, once ABIs stabilize)

## Next steps

1. Expand live route parity to the full mem.* table when sibling needs it.
2. Optional: package a thin “assistant façade” as a dual-surface module once
   ABIs stabilize.
