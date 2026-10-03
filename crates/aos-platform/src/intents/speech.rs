//! Opt-in speech → gate text bus intents (#433).

use crate::subsystem::PlatformSubsystem;
use aos_ipc::BusService;
use aos_proto::speech::{
    intents as speech_intents, SpeechIngestRequest, SpeechTranscribeRequest, ASR_COMMAND_ENV,
};
use aos_proto::AuditAppendRequest;
use std::sync::Arc;

pub fn register(svc: &mut BusService, sub: Arc<PlatformSubsystem>) {
    {
        let s = sub.clone();
        svc.on(speech_intents::INGEST_TRANSCRIPT, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<SpeechIngestRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                if req.actor.is_empty() && ctx.intent.from.starts_with("agent:") {
                    req.actor = ctx.intent.from.clone();
                }
                let resp = s.speech.lock().unwrap().ingest(&req);
                if resp.ok {
                    s.audit(AuditAppendRequest {
                        trace_id: req.trace_id.clone(),
                        actor: req.actor.clone(),
                        action: speech_intents::INGEST_TRANSCRIPT.into(),
                        target: "transcript".into(),
                        detail: serde_json::json!({
                            "source_trust": resp.source_trust,
                            "chars": resp.transcript.len(),
                            "bytes": 0,
                        }),
                    });
                }
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
    {
        let s = sub;
        svc.on(speech_intents::TRANSCRIBE, move |ctx| {
            let s = s.clone();
            async move {
                let mut req = match ctx.payload::<SpeechTranscribeRequest>() {
                    Ok(req) => req,
                    Err(_) => {
                        let _ = ctx
                            .respond_error(aos_ipc::msg::Status::BadRequest, "payload invalide")
                            .await;
                        return;
                    }
                };
                if req.actor.is_empty() && ctx.intent.from.starts_with("agent:") {
                    req.actor = ctx.intent.from.clone();
                }
                let cmd = std::env::var(ASR_COMMAND_ENV).ok();
                let resp = crate::speech::SpeechTranscriptStore::transcribe(&req, cmd.as_deref());
                if resp.ok {
                    s.audit(AuditAppendRequest {
                        trace_id: req.trace_id.clone(),
                        actor: req.actor.clone(),
                        action: speech_intents::TRANSCRIBE.into(),
                        target: req.path.clone(),
                        detail: serde_json::json!({
                            "source_trust": resp.source_trust,
                            "chars": resp.transcript.len(),
                        }),
                    });
                }
                let _ = ctx.respond(aos_ipc::msg::Status::Ok, &resp).await;
            }
        });
    }
}
