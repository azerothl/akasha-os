# Sibling bridge — Akasha OS ↔ Akasha assistant

**Language:** English | [Français](fr/sibling-bridge.md)

> Date: 17/08/2026 · Preview **0.5.0**  
> Status: contract freeze (documentation only)  
> Related: [competitive-landscape.md](competitive-landscape.md), [evolution-roadmap.md](evolution-roadmap.md) E8

## Principle

Do **not** merge [azerothl/akasha](https://github.com/azerothl/akasha) (personal
assistant daemon) and **akasha-os** into one binary. Align schemas so work is
not duplicated blindly; optional “assistant as module” remains a later option.

## OS contracts (source of truth in this repo)

| Area | Live schema | Notes |
|------|-------------|-------|
| Intents / IPC | [`crates/aos-proto`](../crates/aos-proto/src/lib.rs) + CBOR bus | Typed intents; `Intent.from` is caller identity |
| Memory | `mem.*` + E6 relations + E14 `mem.extract` | Flat episodic JSONL + `relations.jsonl`; chat→LT extract opt-in |
| Secrets | `secrets.get` / `set` / `list` | Encrypted vault; raw values only for services (`platformd`, `modeld`, `agentd`) |
| WASM ABI | [`module_rt`](../crates/aos-platform/src/module_rt.rs) + [`modules/sdk`](../modules/sdk) | Dual-surface (tools + UI); `host_call` only; `secrets.get` prohibited in guest |

## Mapping (conceptual)

| Sibling (assistant) | Akasha OS | Align? |
|---------------------|-----------|--------|
| Typed LT memory graph | E6 `MemRelationKind` | Yes — names aligned; storage differs |
| Vault / keyring | E7 `vault.enc` + OS protect | Yes — never raw keys to agents |
| Wasmtime plugins (tools-only) | Dual-surface `.aospkg` | Partial — different contract; do not unify ABIs yet |
| Chat channels / 24/7 gateway | Out of OS core | Leave to sibling |
| Cron / calendar | OS `schedule.*` (E2) | Complementary |
| Auto fact extraction | E14 `mem.extract` | Conceptual peer; OS path is bus-native |

## Non-goals

- Shared process or single installer
- Reimplementing OpenClaw-class channels inside the OS
- Copy-paste of sibling source into this tree

## Next steps (post-0.5)

1. Publish a short CBOR/JSON Schema export of `aos-proto` memory + secrets intents (incl. `mem.extract`).
2. Document HTTP ↔ bus adapters only if a bridge daemon is scheduled.
3. Optional: package a thin “assistant façade” as a dual-surface module once ABIs stabilize.
