//! Path A mix host: preset catalogue + confirmable batches. No DAW, no audio.

use aos_proto::mix::{
    demo_mix_state, validate_mix_state, MixApplyRequest, MixApplyResponse, MixCatalogPreset,
    MixCatalogResponse, MixChange, MixIngestRequest, MixIngestResponse, MixProposalBatch,
    MixProposeRequest, MixProposeResponse, MIX_APPLY_TOOL,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const CATALOG_JSON: &str = include_str!("../data/mix-presets.v1.json");

#[derive(Clone)]
struct CatalogPreset {
    name: String,
    description: String,
    values: Map<String, Value>,
}

pub struct MixCatalog {
    version: u32,
    presets: HashMap<String, CatalogPreset>,
}

impl MixCatalog {
    pub fn builtin() -> Result<Self, String> {
        Self::from_json(CATALOG_JSON)
    }

    pub fn from_json(raw: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
        let version = v
            .get("catalog_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "catalog_version required".to_string())? as u32;
        if version != 1 {
            return Err(format!("unsupported catalog_version {version}"));
        }
        let presets_obj = v
            .get("presets")
            .and_then(Value::as_object)
            .ok_or_else(|| "presets required".to_string())?;
        if presets_obj.is_empty() {
            return Err("catalogue needs at least one preset".into());
        }
        let mut presets = HashMap::new();
        for (name, item) in presets_obj {
            let description = item
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let values = item
                .get("values")
                .and_then(Value::as_object)
                .cloned()
                .ok_or_else(|| format!("preset {name} missing values"))?;
            presets.insert(
                name.clone(),
                CatalogPreset {
                    name: name.clone(),
                    description,
                    values,
                },
            );
        }
        Ok(Self { version, presets })
    }

    pub fn list(&self) -> MixCatalogResponse {
        let mut presets: Vec<MixCatalogPreset> = self
            .presets
            .values()
            .map(|p| MixCatalogPreset {
                name: p.name.clone(),
                description: p.description.clone(),
            })
            .collect();
        presets.sort_by(|a, b| a.name.cmp(&b.name));
        MixCatalogResponse {
            ok: true,
            catalog_version: self.version,
            presets,
            message: None,
        }
    }
}

#[derive(Default)]
struct MixSession {
    descriptors: Option<Value>,
    current: Map<String, Value>,
    pending: Option<MixProposalBatch>,
    last_applied: Option<MixProposalBatch>,
}

#[derive(Default)]
pub struct MixSessionStore {
    catalog: Option<MixCatalog>,
    sessions: HashMap<String, MixSession>,
}

fn session_key(session_id: &str) -> String {
    let t = session_id.trim();
    if t.is_empty() {
        "default".into()
    } else {
        t.to_string()
    }
}

fn envelope_hash(batch: &MixProposalBatch) -> String {
    let payload = json!({
        "tool": MIX_APPLY_TOOL,
        "status": batch.status,
        "reason": batch.reason,
        "changes": batch.changes,
        "undo": batch.undo,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn apply_changes(settings: &mut Map<String, Value>, changes: &[MixChange]) {
    for change in changes {
        settings.insert(change.target.clone(), change.after.clone());
    }
}

impl MixSessionStore {
    pub fn catalog(&mut self) -> Result<MixCatalogResponse, String> {
        if self.catalog.is_none() {
            self.catalog = Some(MixCatalog::builtin()?);
        }
        Ok(self.catalog.as_ref().expect("catalog loaded").list())
    }

    pub fn demo_state() -> Value {
        demo_mix_state()
    }

    pub fn ingest(&mut self, req: &MixIngestRequest) -> MixIngestResponse {
        let session_id = session_key(&req.session_id);
        match validate_mix_state(&req.state) {
            Ok(state) => {
                let sid = state
                    .get("session_id")
                    .and_then(Value::as_str)
                    .unwrap_or(&session_id)
                    .to_string();
                let key = session_key(&sid);
                let entry = self.sessions.entry(key.clone()).or_default();
                entry.descriptors = Some(state);
                MixIngestResponse {
                    ok: true,
                    session_id: key,
                    message: Some("descriptors stored (host-computed; no audio in guest)".into()),
                }
            }
            Err(msg) => MixIngestResponse {
                ok: false,
                session_id,
                message: Some(msg),
            },
        }
    }

    pub fn propose(&mut self, req: &MixProposeRequest) -> MixProposeResponse {
        let session_id = session_key(&req.session_id);
        if !(0.0..=1.0).contains(&req.min_confidence) {
            return MixProposeResponse {
                ok: false,
                session_id,
                batch: MixProposalBatch::abstain("min_confidence must be in [0, 1]"),
                envelope_hash: None,
                message: Some("min_confidence must be in [0, 1]".into()),
            };
        }
        if self.catalog.is_none() {
            match MixCatalog::builtin() {
                Ok(c) => self.catalog = Some(c),
                Err(e) => {
                    return MixProposeResponse {
                        ok: false,
                        session_id,
                        batch: MixProposalBatch::abstain(e.clone()),
                        envelope_hash: None,
                        message: Some(e),
                    };
                }
            }
        }
        let catalog = self.catalog.as_ref().expect("catalog loaded");
        let entry = self.sessions.entry(session_id.clone()).or_default();
        let batch = if req.confidence < req.min_confidence {
            MixProposalBatch::abstain(format!(
                "choice confidence {:.3} below min_confidence {:.3}",
                req.confidence, req.min_confidence
            ))
        } else {
            match catalog.presets.get(&req.preset) {
                None => MixProposalBatch::abstain(format!("unknown preset '{}'", req.preset)),
                Some(preset) => {
                    let mut changes = Vec::new();
                    let mut sources = Map::new();
                    for (target, after) in &preset.values {
                        let before = entry.current.get(target).cloned().unwrap_or(Value::Null);
                        if &before == after {
                            continue;
                        }
                        changes.push(MixChange {
                            target: target.clone(),
                            before,
                            after: after.clone(),
                        });
                        sources.insert(target.clone(), json!(format!("preset:{}", preset.name)));
                    }
                    if changes.is_empty() {
                        MixProposalBatch::abstain("proposed settings match current values")
                    } else {
                        MixProposalBatch::ready("mapped from catalogue", changes, sources)
                    }
                }
            }
        };
        let hash = if batch.executable() {
            Some(envelope_hash(&batch))
        } else {
            None
        };
        entry.pending = Some(batch.clone());
        MixProposeResponse {
            ok: true,
            session_id,
            batch,
            envelope_hash: hash,
            message: None,
        }
    }

    pub fn apply(&mut self, req: &MixApplyRequest, has_apply_cap: bool) -> MixApplyResponse {
        let session_id = session_key(&req.session_id);
        if !has_apply_cap {
            return MixApplyResponse {
                ok: false,
                session_id,
                applied: vec![],
                undo: vec![],
                settings: Map::new(),
                message: Some("permission refusée: mix.apply:* (fail-closed)".into()),
            };
        }
        if !req.confirmation_given {
            return MixApplyResponse {
                ok: false,
                session_id,
                applied: vec![],
                undo: vec![],
                settings: Map::new(),
                message: Some("confirmation required before mix.apply (T7)".into()),
            };
        }
        let entry = self.sessions.entry(session_id.clone()).or_default();
        let pending = match entry.pending.clone() {
            Some(b) if b.executable() => b,
            Some(b) => {
                return MixApplyResponse {
                    ok: false,
                    session_id,
                    applied: vec![],
                    undo: vec![],
                    settings: entry.current.clone(),
                    message: Some(format!("nothing to apply ({})", b.reason)),
                };
            }
            None => {
                return MixApplyResponse {
                    ok: false,
                    session_id,
                    applied: vec![],
                    undo: vec![],
                    settings: entry.current.clone(),
                    message: Some("no pending mix proposal".into()),
                };
            }
        };
        apply_changes(&mut entry.current, &pending.changes);
        entry.last_applied = Some(pending.clone());
        entry.pending = None;
        MixApplyResponse {
            ok: true,
            session_id,
            applied: pending.changes.clone(),
            undo: pending.undo.clone(),
            settings: entry.current.clone(),
            message: Some(
                "session settings updated in host memory; DAW remains outside the sandbox".into(),
            ),
        }
    }

    pub fn undo(&mut self, req: &MixApplyRequest, has_apply_cap: bool) -> MixApplyResponse {
        let session_id = session_key(&req.session_id);
        if !has_apply_cap {
            return MixApplyResponse {
                ok: false,
                session_id,
                applied: vec![],
                undo: vec![],
                settings: Map::new(),
                message: Some("permission refusée: mix.apply:* (fail-closed)".into()),
            };
        }
        let entry = self.sessions.entry(session_id.clone()).or_default();
        let last = match entry.last_applied.clone() {
            Some(b) => b,
            None => {
                return MixApplyResponse {
                    ok: false,
                    session_id,
                    applied: vec![],
                    undo: vec![],
                    settings: entry.current.clone(),
                    message: Some("nothing to undo".into()),
                };
            }
        };
        apply_changes(&mut entry.current, &last.undo);
        entry.last_applied = None;
        MixApplyResponse {
            ok: true,
            session_id,
            applied: last.undo.clone(),
            undo: vec![],
            settings: entry.current.clone(),
            message: Some("one-action undo applied (inverse of last batch)".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::mix::MixIngestRequest;

    #[test]
    fn propose_confirm_undo() {
        let mut store = MixSessionStore::default();
        let ingest = store.ingest(&MixIngestRequest {
            session_id: "s1".into(),
            state: MixSessionStore::demo_state(),
            actor: String::new(),
            caps: vec![],
            trace_id: String::new(),
        });
        assert!(ingest.ok);

        let ready = store.propose(&MixProposeRequest {
            session_id: "s1".into(),
            preset: "vocal_forward".into(),
            confidence: 0.9,
            min_confidence: 0.55,
            actor: String::new(),
            caps: vec![],
            trace_id: String::new(),
        });
        assert_eq!(ready.batch.status, "ready");
        assert!(ready.envelope_hash.is_some());

        let denied = store.apply(
            &MixApplyRequest {
                session_id: "s1".into(),
                confirmation_given: true,
                actor: String::new(),
                caps: vec![],
                trace_id: String::new(),
            },
            false,
        );
        assert!(!denied.ok);

        let no_confirm = store.apply(
            &MixApplyRequest {
                session_id: "s1".into(),
                confirmation_given: false,
                actor: String::new(),
                caps: vec![],
                trace_id: String::new(),
            },
            true,
        );
        assert!(!no_confirm.ok);

        let applied = store.apply(
            &MixApplyRequest {
                session_id: "s1".into(),
                confirmation_given: true,
                actor: String::new(),
                caps: vec![],
                trace_id: String::new(),
            },
            true,
        );
        assert!(applied.ok);
        assert_eq!(applied.settings["vocal.gain_db"], json!(3.0));

        let undone = store.undo(
            &MixApplyRequest {
                session_id: "s1".into(),
                confirmation_given: false,
                actor: String::new(),
                caps: vec![],
                trace_id: String::new(),
            },
            true,
        );
        assert!(undone.ok);
        assert_eq!(undone.settings.get("vocal.gain_db"), Some(&Value::Null));
    }

    #[test]
    fn low_confidence_abstains() {
        let mut store = MixSessionStore::default();
        let resp = store.propose(&MixProposeRequest {
            session_id: "s1".into(),
            preset: "balanced".into(),
            confidence: 0.2,
            min_confidence: 0.55,
            actor: String::new(),
            caps: vec![],
            trace_id: String::new(),
        });
        assert_eq!(resp.batch.status, "abstain");
        assert!(resp.batch.changes.is_empty());
    }
}
