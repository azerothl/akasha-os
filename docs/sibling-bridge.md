# Sibling bridge — Akasha OS ↔ Akasha assistant

**Language:** English | [Français](fr/sibling-bridge.md)

> Date: 17/08/2026 · Preview **0.6.0**  
> Status: contract freeze + JSON Schema export (no live daemon)  
> Related: [competitive-landscape.md](competitive-landscape.md), [evolution-roadmap.md](evolution-roadmap.md) E8, [docs/bridge/](bridge/)

## Principle

Do **not** merge [azerothl/akasha](https://github.com/azerothl/akasha) (personal
assistant daemon) and **akasha-os** into one binary. Align schemas so work is
not duplicated blindly; optional “assistant as module” remains a later option.

## OS contracts (source of truth in this repo)

| Area | Live schema | Notes |
|------|-------------|-------|
| Intents / IPC | [`crates/aos-proto`](../crates/aos-proto/src/lib.rs) + CBOR bus | Typed intents; `Intent.from` is caller identity |
| JSON Schema export | [`docs/bridge/`](bridge/) | Draft-07 JSON view of `mem.*` and `secrets.*` payloads |
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

## HTTP JSON ↔ CBOR intent mapping (contract only)

Preview **0.6.0 does not run** an HTTP adapter inside the OS process. A future
bridge daemon (separate binary, optional) MAY expose JSON over HTTP and
forward to the CBOR bus. Until that daemon exists, this table is the freeze.

Assumed adapter surface (not shipped):

- Base URL: `http://127.0.0.1:<bridge-port>/v1`
- Request body: JSON matching [`docs/bridge/`](bridge/)
- Response body: JSON of the same `aos-proto` type the bus would CBOR-encode
- Identity: header `X-Aos-From` → bus `Intent.from` (do **not** trust a JSON
  `actor` field when it diverges)
- Errors: HTTP 403 if `Intent.from` is an agent and the intent is `secrets.get`

| HTTP | Intent | JSON body schema (`$defs`) | Notes |
|------|--------|----------------------------|-------|
| `POST /mem/working_set` | `mem.working_set` | `MemWorkingRequest` | |
| `POST /mem/working_get` | `mem.working_get` | `MemWorkingRequest` | `messages` ignored |
| `POST /mem/episodic_write` | `mem.episodic_write` | `MemEpisodicWriteRequest` | |
| `POST /mem/episodic_query` | `mem.episodic_query` | `MemEpisodicQueryRequest` | |
| `POST /mem/episodic_delete` | `mem.episodic_delete` | `MemEpisodicDeleteRequest` | |
| `POST /mem/stats` | `mem.stats` | (empty object) | Response: `MemStats` |
| `POST /mem/shared_read` | `mem.shared_read` | `MemSharedReadRequest` | |
| `POST /mem/shared_write` | `mem.shared_write` | `MemSharedWriteRequest` | |
| `POST /mem/user/remember` | `mem.user.remember` | `MemUserRememberRequest` | Response: `MemRememberResponse` |
| `POST /mem/user/recall` | `mem.user.recall` | `MemUserRecallRequest` | |
| `POST /mem/context` | `mem.context` | `MemContextRequest` | Response: `MemContextResponse` |
| `POST /mem/extract` | `mem.extract` | `MemExtractRequest` | Response: `MemExtractResponse` |
| `POST /mem/relate` | `mem.relate` | `MemRelateRequest` | |
| `POST /mem/unrelate` | `mem.unrelate` | `MemUnrelateRequest` | |
| `POST /mem/neighbors` | `mem.neighbors` | `MemNeighborsRequest` | |
| `POST /mem/list` | `mem.list` | `MemListRequest` | |
| `POST /mem/update` | `mem.update` | `MemUpdateRequest` | |
| `POST /secrets/get` | `secrets.get` | `SecretGetRequest` | Services only; 403 for agents |
| `POST /secrets/set` | `secrets.set` | `SecretSetRequest` | Empty `value` deletes |
| `POST /secrets/list` | `secrets.list` | `SecretListRequest` | Names only; Response: `SecretListResponse` |

Wire note: OS daemons speak **CBOR**. The adapter would transcode JSON ↔ CBOR
1:1 using the same serde field names. Do not invent a second payload dialect.

## Non-goals

- Shared process or single installer
- A live HTTP daemon inside `aos-session` / `aos-platformd` in Preview 0.6
- Reimplementing OpenClaw-class channels inside the OS
- Copy-paste of sibling source into this tree
- “Assistant as module” façade (post-0.6, once ABIs stabilize)

## Next steps (post-0.6)

1. Implement a **separate** bridge daemon only if one is scheduled — do not
   fold HTTP into the OS kernel services.
2. Optional: package a thin “assistant façade” as a dual-surface module once
   ABIs stabilize.
