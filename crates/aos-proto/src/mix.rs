//! Mix Path A host contracts (akasha-model T1/T2/T7).
//!
//! Descriptors are host-computed JSON. No WAV/mel in the guest. Applying
//! faders stays outside the sandbox (DAW host); this crate only validates
//! state and shapes confirmable batches.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

pub mod intents {
    pub const CATALOG: &str = "mix.catalog";
    pub const INGEST_STATE: &str = "mix.ingest_state";
    pub const DEMO_STATE: &str = "mix.demo_state";
    pub const LOAD_DEMO: &str = "mix.load_demo";
    pub const PROPOSE: &str = "mix.propose";
    pub const APPLY: &str = "mix.apply";
    pub const UNDO: &str = "mix.undo";
}

pub const MIX_SCHEMA_VERSION: u32 = 1;
pub const MIX_READ_CAP: &str = "mix.read:*";
pub const MIX_APPLY_CAP: &str = "mix.apply:*";
pub const MIX_APPLY_TOOL: &str = "mix.apply_batch";
pub const TRACK_ID_MAX: usize = 64;
pub const TRACK_MAX: usize = 64;

pub fn mix_read_cap() -> &'static str {
    MIX_READ_CAP
}

pub fn mix_apply_cap() -> &'static str {
    MIX_APPLY_CAP
}

fn finite_in_range(value: f64, low: f64, high: f64, name: &str) -> Result<f64, String> {
    if !value.is_finite() {
        return Err(format!("{name} must be finite"));
    }
    if value < low || value > high {
        return Err(format!("{name}={value} outside [{low}, {high}]"));
    }
    Ok(value)
}

fn track_id_ok(id: &str) -> bool {
    if id.is_empty() || id.len() > TRACK_ID_MAX {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

/// Validate host-computed mix descriptors (schema_version 1). Fail closed.
pub fn validate_mix_state(value: &Value) -> Result<Value, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "mix state must be an object".to_string())?;
    let version = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "schema_version required".to_string())?;
    if version != u64::from(MIX_SCHEMA_VERSION) {
        return Err(format!(
            "unsupported schema_version {version}; host speaks {MIX_SCHEMA_VERSION}"
        ));
    }
    let session_id = obj
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "session_id must be a non-empty string".to_string())?;
    if session_id.trim().is_empty() {
        return Err("session_id must be a non-empty string".into());
    }
    if let Some(sr) = obj.get("sample_rate_hz") {
        let hz = sr
            .as_u64()
            .ok_or_else(|| "sample_rate_hz must be an integer".to_string())?;
        if !(8000..=192_000).contains(&hz) {
            return Err(format!("sample_rate_hz={hz} outside [8000, 192000]"));
        }
    }
    let tracks = obj
        .get("tracks")
        .and_then(Value::as_array)
        .ok_or_else(|| "tracks must be a non-empty array".to_string())?;
    if tracks.is_empty() || tracks.len() > TRACK_MAX {
        return Err(format!("tracks length must be in [1, {TRACK_MAX}]"));
    }
    let mut ids = HashSet::new();
    for (i, track) in tracks.iter().enumerate() {
        let t = track
            .as_object()
            .ok_or_else(|| format!("tracks[{i}] must be an object"))?;
        let tid = t
            .get("track_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("tracks[{i}].track_id required"))?;
        if !track_id_ok(tid) {
            return Err(format!("tracks[{i}].track_id invalid"));
        }
        if !ids.insert(tid.to_string()) {
            return Err(format!("duplicate track_id {tid}"));
        }
        finite_in_range(
            t.get("rms_db")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("tracks[{i}].rms_db required"))?,
            -120.0,
            0.0,
            &format!("tracks[{i}].rms_db"),
        )?;
        finite_in_range(
            t.get("lufs")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("tracks[{i}].lufs required"))?,
            -70.0,
            0.0,
            &format!("tracks[{i}].lufs"),
        )?;
        finite_in_range(
            t.get("spectral_centroid_hz")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("tracks[{i}].spectral_centroid_hz required"))?,
            20.0,
            20_000.0,
            &format!("tracks[{i}].spectral_centroid_hz"),
        )?;
        let bands = t
            .get("band_energies_db")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("tracks[{i}].band_energies_db required"))?;
        if bands.len() != 4 {
            return Err(format!("tracks[{i}].band_energies_db must have 4 bands"));
        }
        for (b, band) in bands.iter().enumerate() {
            finite_in_range(
                band.as_f64()
                    .ok_or_else(|| format!("tracks[{i}].band_energies_db[{b}]"))?,
                -120.0,
                0.0,
                &format!("tracks[{i}].band_energies_db[{b}]"),
            )?;
        }
        finite_in_range(
            t.get("crest_factor_db")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("tracks[{i}].crest_factor_db required"))?,
            0.0,
            40.0,
            &format!("tracks[{i}].crest_factor_db"),
        )?;
        finite_in_range(
            t.get("stereo_correlation")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("tracks[{i}].stereo_correlation required"))?,
            -1.0,
            1.0,
            &format!("tracks[{i}].stereo_correlation"),
        )?;
    }
    if let Some(pairs) = obj.get("pair_masking") {
        let arr = pairs
            .as_array()
            .ok_or_else(|| "pair_masking must be an array".to_string())?;
        for (i, pair) in arr.iter().enumerate() {
            let p = pair
                .as_object()
                .ok_or_else(|| format!("pair_masking[{i}] must be an object"))?;
            let masker = p
                .get("masker_id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("pair_masking[{i}].masker_id required"))?;
            let maskee = p
                .get("maskee_id")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("pair_masking[{i}].maskee_id required"))?;
            if !ids.contains(masker) || !ids.contains(maskee) {
                return Err(format!("pair_masking[{i}] unknown track id"));
            }
            finite_in_range(
                p.get("masking_index")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("pair_masking[{i}].masking_index required"))?,
                0.0,
                1.0,
                &format!("pair_masking[{i}].masking_index"),
            )?;
        }
    }
    Ok(value.clone())
}

/// Synthetic descriptors for the Path A UI stub. Not measured audio.
pub fn demo_mix_state() -> Value {
    json!({
        "schema_version": MIX_SCHEMA_VERSION,
        "session_id": "demo-mix-1",
        "sample_rate_hz": 48000,
        "tracks": [
            {
                "track_id": "vocal",
                "rms_db": -18.0,
                "lufs": -14.0,
                "spectral_centroid_hz": 1800.0,
                "band_energies_db": [-22.0, -16.0, -14.0, -20.0],
                "crest_factor_db": 12.0,
                "stereo_correlation": 0.15
            },
            {
                "track_id": "bass",
                "rms_db": -12.0,
                "lufs": -11.0,
                "spectral_centroid_hz": 120.0,
                "band_energies_db": [-8.0, -14.0, -28.0, -40.0],
                "crest_factor_db": 6.0,
                "stereo_correlation": 0.92
            }
        ],
        "pair_masking": [
            {"masker_id": "bass", "maskee_id": "vocal", "masking_index": 0.35}
        ]
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MixChange {
    pub target: String,
    pub before: Value,
    pub after: Value,
}

impl MixChange {
    pub fn inverted(&self) -> Self {
        Self {
            target: self.target.clone(),
            before: self.after.clone(),
            after: self.before.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct MixProposalBatch {
    pub status: String,
    pub reason: String,
    #[serde(default)]
    pub changes: Vec<MixChange>,
    #[serde(default)]
    pub undo: Vec<MixChange>,
    #[serde(default)]
    pub sources: Map<String, Value>,
}

impl MixProposalBatch {
    pub fn abstain(reason: impl Into<String>) -> Self {
        Self {
            status: "abstain".into(),
            reason: reason.into(),
            changes: vec![],
            undo: vec![],
            sources: Map::new(),
        }
    }

    pub fn ready(
        reason: impl Into<String>,
        changes: Vec<MixChange>,
        sources: Map<String, Value>,
    ) -> Self {
        let undo: Vec<MixChange> = changes.iter().rev().map(MixChange::inverted).collect();
        Self {
            status: "ready".into(),
            reason: reason.into(),
            changes,
            undo,
            sources,
        }
    }

    pub fn executable(&self) -> bool {
        self.status == "ready" && !self.changes.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MixCatalogPreset {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MixCatalogResponse {
    pub ok: bool,
    pub catalog_version: u32,
    #[serde(default)]
    pub presets: Vec<MixCatalogPreset>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MixIngestRequest {
    #[serde(default)]
    pub session_id: String,
    /// Host-computed descriptor JSON (no audio bytes).
    pub state: Value,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MixIngestResponse {
    pub ok: bool,
    pub session_id: String,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MixProposeRequest {
    #[serde(default)]
    pub session_id: String,
    pub preset: String,
    pub confidence: f64,
    pub min_confidence: f64,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MixProposeResponse {
    pub ok: bool,
    pub session_id: String,
    pub batch: MixProposalBatch,
    #[serde(default)]
    pub envelope_hash: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct MixApplyRequest {
    #[serde(default)]
    pub session_id: String,
    /// Must be true — fail-closed otherwise (T7 confirmation).
    #[serde(default)]
    pub confirmation_given: bool,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MixApplyResponse {
    pub ok: bool,
    pub session_id: String,
    #[serde(default)]
    pub applied: Vec<MixChange>,
    #[serde(default)]
    pub undo: Vec<MixChange>,
    #[serde(default)]
    pub settings: Map<String, Value>,
    #[serde(default)]
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_state_validates() {
        validate_mix_state(&demo_mix_state()).unwrap();
    }

    #[test]
    fn oob_rms_fails_closed() {
        let mut s = demo_mix_state();
        s["tracks"][0]["rms_db"] = json!(1.0);
        assert!(validate_mix_state(&s).is_err());
    }

    #[test]
    fn duplicate_track_fails() {
        let mut s = demo_mix_state();
        s["tracks"][1]["track_id"] = json!("vocal");
        assert!(validate_mix_state(&s).is_err());
    }

    #[test]
    fn undo_inverts_batch() {
        let batch = MixProposalBatch::ready(
            "mapped",
            vec![MixChange {
                target: "vocal.gain_db".into(),
                before: json!(0.0),
                after: json!(3.0),
            }],
            Map::new(),
        );
        assert_eq!(batch.undo[0].after, json!(0.0));
        assert!(batch.executable());
    }
}
