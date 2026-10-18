//! Shared, audited boundary for subtree proposals and confirmed host application.
use crate::{subsystem::PlatformSubsystem, workspace_refactor::Request};
use aos_proto::AuditAppendRequest;
use serde_json::{json, Value};

pub async fn dispatch(
    sub: &PlatformSubsystem,
    intent: &str,
    args: Value,
    actor: &str,
    caps: &[String],
    trace: &str,
) -> Result<Value, String> {
    let target = args["root"].as_str().unwrap_or("").to_owned();
    let result = execute(sub, intent, args, actor, caps).await;
    sub.audit(AuditAppendRequest { trace_id: trace.into(), actor: actor.into(), action: intent.into(), target,
        detail: json!({"ok":result.is_ok(),"error":result.as_ref().err(),"plan_id":result.as_ref().ok().and_then(|r|r.get("plan_id"))}) });
    result
}
async fn execute(
    sub: &PlatformSubsystem,
    intent: &str,
    args: Value,
    actor: &str,
    caps: &[String],
) -> Result<Value, String> {
    let req: Request = serde_json::from_value(args).map_err(|e| e.to_string())?;
    if matches!(intent, "refactor.apply" | "refactor.undo") {
        let review = {
            let mgr = sub.workspaces.lock().unwrap();
            sub.refactors
                .lock()
                .unwrap()
                .review(&mgr, &req, actor, caps)?
        };
        if req.review_sha256.as_deref() != review["review_sha256"].as_str() {
            return Err("inspect the current refactor review before requesting application".into());
        }
        let allowed = if intent == "refactor.undo" {
            "can_undo"
        } else {
            "can_apply"
        };
        if review[allowed] != true {
            return Err("refactor state does not permit this operation".into());
        }
        let (_, answer) = sub.confirm.ask(actor.into(), intent.into(), req.root.clone(),
            format!("Autoriser {intent} pour le plan {} dans {} ? Revue SHA-256 : {}. Les fichiers proposes seront modifies sur votre disque.",
                req.plan_id.as_deref().unwrap_or(""), req.root, req.review_sha256.as_deref().unwrap_or("")), Some(120)).await;
        if !answer.await.map(|r| r.approved).unwrap_or(false) {
            return Err("confirmation denied or expired".into());
        }
    }
    // No locks are held while the human reviews. The store checks identity, review
    // fingerprint, capabilities, binding and current contents again before writes.
    let mgr = sub.workspaces.lock().unwrap();
    let mut store = sub.refactors.lock().unwrap();
    match intent {
        "refactor.create" => store.create(&mgr, &req, actor, caps),
        "refactor.submit" => store.submit(&mgr, &req, actor, caps),
        "refactor.review" => store.review(&mgr, &req, actor, caps),
        "refactor.apply" => store.apply(&mgr, &req, actor, caps, false),
        "refactor.undo" => store.apply(&mgr, &req, actor, caps, true),
        _ => Err("unsupported refactor intent".into()),
    }
}
