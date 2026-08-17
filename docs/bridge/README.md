# Sibling bridge schemas

**Language:** English | [Français](../fr/sibling-bridge.md)

JSON Schema (draft-07) export of Akasha OS `aos-proto` payloads for the
sibling [Akasha](https://github.com/azerothl/akasha) assistant. Canonical
wire format on the OS remains **CBOR on the intent bus**. These files are a
JSON view so an HTTP adapter can stay aligned without sharing a process.

| File | Intents |
|------|---------|
| [aos-proto-memory.json](aos-proto-memory.json) | `mem.*` including E6 relations and E14 `mem.extract` |
| [aos-proto-secrets.json](aos-proto-secrets.json) | `secrets.get` / `set` / `list` |

Regenerate (must stay in sync with `crates/aos-proto`):

```powershell
$env:UPDATE_BRIDGE_SCHEMAS = "1"
cargo test -p aos-proto committed_bridge_schemas_match
```

See [sibling-bridge.md](../sibling-bridge.md) for the HTTP JSON ↔ CBOR mapping
(contract only — no live daemon in Preview 0.6.0).
