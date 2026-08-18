//! JSON Schema export for the sibling bridge (E8 / Preview 0.6).
//!
//! Canonical wire format on Akasha OS remains **CBOR on the intent bus**.
//! These documents are a JSON view of the same `aos-proto` payloads so a
//! sibling HTTP adapter can stay aligned without sharing a process.

use schemars::{schema_for, JsonSchema};
use serde_json::{json, Map, Value};

use crate::{
    decl_ui::{DeclUiDocument, DeclUiWidget, ModuleUiResponse},
    MemContextRequest, MemContextResponse, MemEpisodicDeleteRequest, MemEpisodicQueryRequest,
    MemEpisodicWriteRequest, MemExtractOutcome, MemExtractOutcomeKind, MemExtractRequest,
    MemExtractResponse, MemExtractedFact, MemHit, MemListRequest, MemNeighborsRequest,
    MemRelateRequest, MemRelation, MemRelationKind, MemRememberResponse, MemSharedReadRequest,
    MemSharedWriteRequest, MemStats, MemUnrelateRequest, MemUpdateRequest, MemUserRecallRequest,
    MemUserRememberRequest, MemWorkingRequest, SecretGetRequest, SecretListRequest,
    SecretListResponse, SecretSetRequest,
};

const SCHEMA_META: &str = "http://json-schema.org/draft-07/schema#";

fn schema_of<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).expect("json schema")
}

fn defs(pairs: &[(&str, Value)]) -> Value {
    let mut items: Vec<(String, Value)> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut map = Map::new();
    for (k, v) in items {
        map.insert(k, v);
    }
    Value::Object(map)
}

/// Pretty JSON with a trailing newline (stable for `docs/bridge/` commits).
pub fn to_pretty(value: &Value) -> String {
    let mut s = serde_json::to_string_pretty(value).expect("pretty json");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Combined JSON Schema for `mem.*` intent payloads.
pub fn memory_schema_document() -> Value {
    json!({
        "$schema": SCHEMA_META,
        "title": "Akasha OS aos-proto memory intents",
        "description": "JSON Schema (draft-07) for mem.* bus payloads, including E6 relations and E14 mem.extract. Canonical wire format remains CBOR on the OS intent bus.",
        "version": "0.6.0",
        "$defs": defs(&[
            ("MemContextRequest", schema_of::<MemContextRequest>()),
            ("MemContextResponse", schema_of::<MemContextResponse>()),
            ("MemEpisodicDeleteRequest", schema_of::<MemEpisodicDeleteRequest>()),
            ("MemEpisodicQueryRequest", schema_of::<MemEpisodicQueryRequest>()),
            ("MemEpisodicWriteRequest", schema_of::<MemEpisodicWriteRequest>()),
            ("MemExtractOutcome", schema_of::<MemExtractOutcome>()),
            ("MemExtractOutcomeKind", schema_of::<MemExtractOutcomeKind>()),
            ("MemExtractRequest", schema_of::<MemExtractRequest>()),
            ("MemExtractResponse", schema_of::<MemExtractResponse>()),
            ("MemExtractedFact", schema_of::<MemExtractedFact>()),
            ("MemHit", schema_of::<MemHit>()),
            ("MemListRequest", schema_of::<MemListRequest>()),
            ("MemNeighborsRequest", schema_of::<MemNeighborsRequest>()),
            ("MemRelateRequest", schema_of::<MemRelateRequest>()),
            ("MemRelation", schema_of::<MemRelation>()),
            ("MemRelationKind", schema_of::<MemRelationKind>()),
            ("MemRememberResponse", schema_of::<MemRememberResponse>()),
            ("MemSharedReadRequest", schema_of::<MemSharedReadRequest>()),
            ("MemSharedWriteRequest", schema_of::<MemSharedWriteRequest>()),
            ("MemStats", schema_of::<MemStats>()),
            ("MemUnrelateRequest", schema_of::<MemUnrelateRequest>()),
            ("MemUpdateRequest", schema_of::<MemUpdateRequest>()),
            ("MemUserRecallRequest", schema_of::<MemUserRecallRequest>()),
            ("MemUserRememberRequest", schema_of::<MemUserRememberRequest>()),
            ("MemWorkingRequest", schema_of::<MemWorkingRequest>()),
        ]),
    })
}

/// Combined JSON Schema for `secrets.*` intent payloads.
pub fn secrets_schema_document() -> Value {
    json!({
        "$schema": SCHEMA_META,
        "title": "Akasha OS aos-proto secrets intents",
        "description": "JSON Schema (draft-07) for secrets.* bus payloads. Raw values are for OS services only (F-SEC-04); never for agents. Canonical wire format remains CBOR.",
        "version": "0.6.0",
        "$defs": defs(&[
            ("SecretGetRequest", schema_of::<SecretGetRequest>()),
            ("SecretListRequest", schema_of::<SecretListRequest>()),
            ("SecretListResponse", schema_of::<SecretListResponse>()),
            ("SecretSetRequest", schema_of::<SecretSetRequest>()),
        ]),
    })
}

/// JSON Schema for E15 declarative module UI documents (`ui/index.html`).
pub fn decl_ui_schema_document() -> Value {
    json!({
        "$schema": SCHEMA_META,
        "title": "Akasha OS declarative module UI (E15)",
        "description": "Closed widget vocabulary for host-rendered module tabs in Preview 0.8. Not HTML/JS.",
        "version": "0.8.0",
        "$defs": defs(&[
            ("DeclUiDocument", schema_of::<DeclUiDocument>()),
            ("DeclUiWidget", schema_of::<DeclUiWidget>()),
            ("ModuleUiResponse", schema_of::<ModuleUiResponse>()),
        ]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn bridge_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/bridge")
    }

    #[test]
    fn committed_bridge_schemas_match() {
        let dir = bridge_dir();
        let mem = to_pretty(&memory_schema_document());
        let sec = to_pretty(&secrets_schema_document());
        let decl = to_pretty(&decl_ui_schema_document());
        if std::env::var("UPDATE_BRIDGE_SCHEMAS").ok().as_deref() == Some("1") {
            fs::create_dir_all(&dir).expect("docs/bridge");
            fs::write(dir.join("aos-proto-memory.json"), &mem).expect("write memory schema");
            fs::write(dir.join("aos-proto-secrets.json"), &sec).expect("write secrets schema");
            fs::write(dir.join("aos-proto-decl-ui.json"), &decl).expect("write decl-ui schema");
            return;
        }
        let got_mem = fs::read_to_string(dir.join("aos-proto-memory.json")).unwrap_or_default();
        let got_sec = fs::read_to_string(dir.join("aos-proto-secrets.json")).unwrap_or_default();
        let got_decl = fs::read_to_string(dir.join("aos-proto-decl-ui.json")).unwrap_or_default();
        assert_eq!(
            got_mem, mem,
            "docs/bridge/aos-proto-memory.json is stale; rerun with UPDATE_BRIDGE_SCHEMAS=1"
        );
        assert_eq!(
            got_sec, sec,
            "docs/bridge/aos-proto-secrets.json is stale; rerun with UPDATE_BRIDGE_SCHEMAS=1"
        );
        assert_eq!(
            got_decl, decl,
            "docs/bridge/aos-proto-decl-ui.json is stale; rerun with UPDATE_BRIDGE_SCHEMAS=1"
        );
    }
}
