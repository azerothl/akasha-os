//! Opt-in host ASR → text for the tool gate. No native audio encoder.
//!
//! Preview never ships STT weights. A host command (`AOS_ASR_COMMAND`) or an
//! already-produced transcript may be ingested; the text is untrusted.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod intents {
    pub const INGEST_TRANSCRIPT: &str = "speech.ingest_transcript";
    pub const TRANSCRIBE: &str = "speech.transcribe";
}

pub const SPEECH_INGEST_CAP: &str = "speech.ingest:*";
pub const SPEECH_TRANSCRIBE_CAP: &str = "speech.transcribe:*";
pub const ASR_COMMAND_ENV: &str = "AOS_ASR_COMMAND";
pub const TRANSCRIPT_MAX_CHARS: usize = 8_000;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpeechIngestRequest {
    pub transcript: String,
    #[serde(default)]
    pub typed_text: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpeechIngestResponse {
    pub ok: bool,
    /// Always `untrusted` or `mixed` — never `trusted` for ASR text.
    pub source_trust: String,
    pub transcript: String,
    /// Hint for Path A `evaluate_gate` context (same injection policy as chat).
    pub gate_context: serde_json::Value,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpeechTranscribeRequest {
    /// Host filesystem path to an audio file. Not stored in git.
    pub path: String,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub caps: Vec<String>,
    #[serde(default)]
    pub trace_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpeechTranscribeResponse {
    pub ok: bool,
    #[serde(default)]
    pub transcript: String,
    #[serde(default)]
    pub source_trust: String,
    #[serde(default)]
    pub message: Option<String>,
}

/// Truncate and classify ASR text. Empty transcripts fail closed.
pub fn normalize_transcript(raw: &str) -> Result<String, String> {
    let text = raw.trim();
    if text.is_empty() {
        return Err("transcript must be non-empty".into());
    }
    let mut out = text.to_string();
    if out.chars().count() > TRANSCRIPT_MAX_CHARS {
        out = out.chars().take(TRANSCRIPT_MAX_CHARS).collect();
    }
    Ok(out)
}

/// ASR-only → untrusted; ASR + typed operator text → mixed.
pub fn asr_source_trust(typed_text: &str) -> &'static str {
    if typed_text.trim().is_empty() {
        "untrusted"
    } else {
        "mixed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_fails() {
        assert!(normalize_transcript("  ").is_err());
    }

    #[test]
    fn asr_only_untrusted() {
        assert_eq!(asr_source_trust(""), "untrusted");
        assert_eq!(asr_source_trust("please save"), "mixed");
    }
}
