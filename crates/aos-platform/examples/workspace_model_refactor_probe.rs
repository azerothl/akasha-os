//! Production-host apply/test/undo for two proposals authored by a real local model.
//! Approvals are automated only for this marked, generated fixture, never user projects.
use aos_platform::{
    intents::workspace_execution::dispatch, subsystem::PlatformConfig, PlatformSubsystem,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let prepare_only = args.first().is_some_and(|arg| arg == "--prepare");
    if prepare_only {
        args.remove(0);
    }
    if args.len() != 2 {
        return Err("usage: workspace_model_refactor_probe MARKED_FIXTURE ABSOLUTE_PYTHON".into());
    }
    let fixture = Path::new(&args[0]).canonicalize()?;
    if !fixture.join(".akasha-os-model-probe").is_file() || !fixture.join("verify.py").is_file() {
        return Err("generated model fixture marker/validator missing".into());
    }
    let python = PathBuf::from(&args[1]);
    if !python.is_absolute() || !python.is_file() {
        return Err("existing absolute Python required".into());
    }
    // Check the actual executable before requesting any inference or approval.
    // Windows Store aliases can exist as files but refuse native canonicalization.
    let python = python.canonicalize().map_err(|error| {
        format!("Python must be an accessible installed executable, not a Store alias: {error}")
    })?;
    let project = fixture.join("project").canonicalize()?;
    if project.parent() != Some(fixture.as_path()) {
        return Err("project escapes generated fixture".into());
    }
    let control = fixture.join("control");
    if control.exists() {
        if !control.join(".generated-host-fixture").is_file() {
            return Err("unowned fixture control directory".into());
        }
    } else {
        std::fs::create_dir(&control)?;
        std::fs::write(
            control.join(".generated-host-fixture"),
            "generated host fixture",
        )?;
    }
    let dir = |name: &str| control.join(name).to_string_lossy().into_owned();
    let config = PlatformConfig {
        bus: "ipc://model-proof".into(),
        audit_dir: dir("audit"),
        storage_dir: dir("storage"),
        memory_dir: dir("memory"),
        modules_dir: dir("modules"),
        catalogue_file: dir("absent.yaml"),
        community_catalogue_dir: dir("catalogue"),
        skills_dir: dir("skills"),
        sessions_dir: dir("sessions"),
        embed_model: None,
        policies_file: None,
        confirm_timeout_sec: 10,
        secrets_file: dir("secrets/store"),
        net_mode: "offline_strict".into(),
        memory_v2: false,
        memory_v2_shadow: false,
    };
    let sub = PlatformSubsystem::open(&config)?;
    sub.workspaces
        .lock()
        .unwrap()
        .bind(&project.to_string_lossy(), Some("fixture"), "human:ui")?;
    std::fs::write(control.join("sessions/workspace-process-policy.json"),json!({"workspaces":{"fixture":{
        "baseline":{"program":python,"args":["-I","-S","-B",fixture.join("verify.py")],"timeout_sec":10},
        "verify":{"program":python,"args":["-I","-S","-B",fixture.join("verify.py"),"--require-refactor"],"timeout_sec":10}
    }}}).to_string())?;
    let caps = ["refactor.manage", "fs.read", "fs.write", "process.run"]
        .iter()
        .map(|kind| format!("{kind}:/host/fixture/**"))
        .collect::<Vec<_>>();
    if prepare_only {
        let mut contexts = serde_json::Map::new();
        for part in ["frontend", "backend"] {
            let caps = vec![format!("fs.read:/host/fixture/{part}/**")];
            let context = dispatch(
                &sub,
                "workspace.read",
                json!({"root":"fixture","path":format!("{part}/module.py")}),
                &format!("agent:{part}"),
                &caps,
                "model-proof-context-before-inference",
            )
            .await?;
            contexts.insert(part.into(), context);
        }
        println!("{}", Value::Object(contexts));
        return Ok(());
    }
    let approvals = Arc::new(Mutex::new(Vec::new()));
    let recorded = approvals.clone();
    let confirm = sub.confirm.clone();
    let mut pending = confirm.subscribe().await;
    let approver = tokio::spawn(async move {
        let mut denied_once = false;
        while let Some(request) = pending.recv().await {
            if request.actor != "human:ui"
                || request.target != "fixture"
                || !["refactor.apply", "refactor.undo", "process.run"]
                    .contains(&request.action.as_str())
            {
                break;
            }
            let approved = if request.action == "refactor.apply" && !denied_once {
                denied_once = true;
                false
            } else {
                true
            };
            recorded
                .lock()
                .unwrap()
                .push(json!({"action":request.action,"approved":approved}));
            confirm.respond(&request.id, approved, false).await;
        }
    });
    let baseline = dispatch(
        &sub,
        "process.run",
        json!({"root":"fixture","task":"baseline"}),
        "human:ui",
        &caps,
        "model-proof-baseline",
    )
    .await?;
    if baseline["ok"] != true {
        return Err(format!("baseline failed: {baseline}").into());
    }
    let plan=dispatch(&sub,"refactor.create",json!({"root":"fixture","assignments":[{"part_id":"frontend","actor":"agent:frontend","subtree":"frontend"},{"part_id":"backend","actor":"agent:backend","subtree":"backend"}]}),"human:ui",&caps,"model-proof-plan").await?;
    let plan_id = plan["plan_id"].clone();
    let mut originals = Vec::new();
    let mut submitted = Vec::new();
    for part in ["frontend", "backend"] {
        let actor = format!("agent:{part}");
        let worker_caps = ["fs.read", "refactor.submit"]
            .iter()
            .map(|kind| format!("{kind}:/host/fixture/{part}/**"))
            .collect::<Vec<_>>();
        let path = format!("{part}/module.py");
        let original = dispatch(
            &sub,
            "workspace.read",
            json!({"root":"fixture","path":path}),
            &actor,
            &worker_caps,
            "model-proof-read",
        )
        .await?;
        originals.push((
            path.clone(),
            original["content"]
                .as_str()
                .ok_or("source missing")?
                .to_owned(),
        ));
        let proposal: Value = serde_json::from_slice(&std::fs::read(
            fixture.join(format!("{part}-proposal.json")),
        )?)?;
        if proposal["base_sha256"] != original["sha256"] {
            return Err(
                "fixture source changed after model context was read; refusing stale proposal"
                    .into(),
            );
        }
        let content = proposal["content"]
            .as_str()
            .ok_or("model content missing")?;
        if content == original["content"].as_str().unwrap() {
            return Err("model returned unchanged source".into());
        }
        let request = json!({"root":"fixture","plan_id":plan_id,"part_id":part,"changes":[{"path":path,"base_sha256":proposal["base_sha256"],"content":content}]});
        let wrong = if part == "frontend" {
            "agent:backend"
        } else {
            "agent:frontend"
        };
        assert!(dispatch(
            &sub,
            "refactor.submit",
            request.clone(),
            wrong,
            &worker_caps,
            "model-proof-wrong-actor"
        )
        .await
        .is_err());
        submitted.push(
            dispatch(
                &sub,
                "refactor.submit",
                request,
                &actor,
                &worker_caps,
                "model-proof-submit",
            )
            .await?,
        );
    }
    let review = dispatch(
        &sub,
        "refactor.review",
        json!({"root":"fixture","plan_id":plan_id}),
        "human:ui",
        &caps,
        "model-proof-review",
    )
    .await?;
    std::fs::write(
        fixture.join("review.json"),
        serde_json::to_vec_pretty(&review)?,
    )?;
    let request =
        json!({"root":"fixture","plan_id":plan_id,"review_sha256":review["review_sha256"]});
    assert!(dispatch(
        &sub,
        "refactor.apply",
        request.clone(),
        "human:ui",
        &caps,
        "model-proof-denied"
    )
    .await
    .is_err());
    for (path, original) in &originals {
        assert_eq!(std::fs::read_to_string(project.join(path))?, *original);
    }
    let applied = dispatch(
        &sub,
        "refactor.apply",
        request,
        "human:ui",
        &caps,
        "model-proof-apply",
    )
    .await?;
    let verified = dispatch(
        &sub,
        "process.run",
        json!({"root":"fixture","task":"verify"}),
        "human:ui",
        &caps,
        "model-proof-verify",
    )
    .await;
    // Always restore this generated project, including a failed semantic candidate.
    let review = dispatch(
        &sub,
        "refactor.review",
        json!({"root":"fixture","plan_id":plan_id}),
        "human:ui",
        &caps,
        "model-proof-undo-review",
    )
    .await?;
    let undone = dispatch(
        &sub,
        "refactor.undo",
        json!({"root":"fixture","plan_id":plan_id,"review_sha256":review["review_sha256"]}),
        "human:ui",
        &caps,
        "model-proof-undo",
    )
    .await?;
    for (path, original) in &originals {
        assert_eq!(std::fs::read_to_string(project.join(path))?, *original);
    }
    let verified = verified?;
    if verified["ok"] != true {
        return Err(format!(
            "model-authored refactor failed verification; originals restored: {verified}"
        )
        .into());
    }
    sub.audit.lock().unwrap().verify()?;
    let report = json!({"ok":true,"scope":"native production intents, two actor-scoped real-model proposals, generated fixture only","submitted":submitted,"baseline":baseline,"applied":applied,"verified":verified,"undo":undone,"originals_restored":true,"wrong_actor_denied":true,"denied_confirmation_wrote_nothing":true,"approvals":approvals.lock().unwrap().clone(),"audit_chain_verified":true});
    approver.abort();
    println!("{report}");
    Ok(())
}
