//! Opt-in host speech → text. No bundled STT model.

use aos_proto::speech::{
    asr_source_trust, normalize_transcript, SpeechIngestRequest, SpeechIngestResponse,
    SpeechTranscribeRequest, SpeechTranscribeResponse, ASR_COMMAND_ENV,
};
use serde_json::json;
use std::path::Path;
use std::process::Command;

#[derive(Default)]
pub struct SpeechTranscriptStore {
    last: Option<(String, String)>,
}

impl SpeechTranscriptStore {
    pub fn ingest(&mut self, req: &SpeechIngestRequest) -> SpeechIngestResponse {
        match normalize_transcript(&req.transcript) {
            Err(msg) => SpeechIngestResponse {
                ok: false,
                source_trust: "untrusted".into(),
                transcript: String::new(),
                gate_context: json!({}),
                message: Some(msg),
            },
            Ok(transcript) => {
                let source_trust = asr_source_trust(&req.typed_text);
                self.last = Some((req.actor.clone(), transcript.clone()));
                SpeechIngestResponse {
                    ok: true,
                    source_trust: source_trust.into(),
                    transcript: transcript.clone(),
                    gate_context: json!({
                        "source_trust": source_trust,
                        "sufficient_context": true,
                        "asr_transcript": transcript,
                        "typed_text": req.typed_text,
                        "injection_policy": "same as chat: ASR text is untrusted evidence",
                    }),
                    message: Some(
                        "transcript stored as untrusted/mixed text for evaluate_gate".into(),
                    ),
                }
            }
        }
    }

    pub fn transcribe(
        req: &SpeechTranscribeRequest,
        command: Option<&str>,
    ) -> SpeechTranscribeResponse {
        let env_cmd = std::env::var(ASR_COMMAND_ENV).ok();
        let cmd = match command {
            Some(s) if s.trim().is_empty() => None,
            Some(s) => Some(s.trim()),
            None => env_cmd.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        };
        let Some(cmd) = cmd else {
            return SpeechTranscribeResponse {
                ok: false,
                transcript: String::new(),
                source_trust: "untrusted".into(),
                message: Some(
                    "asr_not_configured: no bundled STT. Set AOS_ASR_COMMAND to an opt-in host transcriber (stdout = transcript), or paste OS STT text into speech.ingest_transcript.".into(),
                ),
            };
        };
        if req.path.trim().is_empty() || !Path::new(&req.path).is_file() {
            return SpeechTranscribeResponse {
                ok: false,
                transcript: String::new(),
                source_trust: "untrusted".into(),
                message: Some("audio path missing or not a file (host FS; not in git)".into()),
            };
        }
        let mut parts = cmd.split_whitespace();
        let bin = match parts.next() {
            Some(b) => b,
            None => {
                return SpeechTranscribeResponse {
                    ok: false,
                    transcript: String::new(),
                    source_trust: "untrusted".into(),
                    message: Some("AOS_ASR_COMMAND empty".into()),
                };
            }
        };
        let mut child = Command::new(bin);
        for arg in parts {
            child.arg(arg);
        }
        child.arg(&req.path);
        let output = match child.output() {
            Ok(o) => o,
            Err(e) => {
                return SpeechTranscribeResponse {
                    ok: false,
                    transcript: String::new(),
                    source_trust: "untrusted".into(),
                    message: Some(format!("ASR command failed to start: {e}")),
                };
            }
        };
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return SpeechTranscribeResponse {
                ok: false,
                transcript: String::new(),
                source_trust: "untrusted".into(),
                message: Some(format!(
                    "ASR command failed: {}",
                    err.trim().chars().take(240).collect::<String>()
                )),
            };
        }
        match normalize_transcript(&String::from_utf8_lossy(&output.stdout)) {
            Ok(transcript) => SpeechTranscribeResponse {
                ok: true,
                transcript,
                source_trust: "untrusted".into(),
                message: Some("host command transcript; treat as untrusted chat text".into()),
            },
            Err(msg) => SpeechTranscribeResponse {
                ok: false,
                transcript: String::new(),
                source_trust: "untrusted".into(),
                message: Some(msg),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_marks_untrusted() {
        let mut store = SpeechTranscriptStore::default();
        let resp = store.ingest(&SpeechIngestRequest {
            transcript: "export my notes".into(),
            typed_text: String::new(),
            actor: "user".into(),
            caps: vec![],
            trace_id: String::new(),
        });
        assert!(resp.ok);
        assert_eq!(resp.source_trust, "untrusted");
        assert_eq!(resp.gate_context["source_trust"], "untrusted");
    }

    #[test]
    fn transcribe_without_command() {
        let resp = SpeechTranscriptStore::transcribe(
            &SpeechTranscribeRequest {
                path: "/tmp/missing.wav".into(),
                actor: String::new(),
                caps: vec![],
                trace_id: String::new(),
            },
            Some(""),
        );
        assert!(!resp.ok);
        assert!(resp.message.unwrap().contains("asr_not_configured"));
    }

    #[test]
    fn transcribe_echo_command() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("aos-asr-stub-{}.txt", std::process::id()));
        std::fs::write(&path, b"not audio, just a host file").unwrap();
        let resp = SpeechTranscriptStore::transcribe(
            &SpeechTranscribeRequest {
                path: path.display().to_string(),
                actor: String::new(),
                caps: vec![],
                trace_id: String::new(),
            },
            Some("echo hello-from-host-stt"),
        );
        let _ = std::fs::remove_file(&path);
        assert!(resp.ok);
        assert!(resp.transcript.contains("hello-from-host-stt"));
        assert_eq!(resp.source_trust, "untrusted");
    }
}
