//! Exercise the production background service and document publication, in an
//! isolated platform store. Does NOT claim to test desktop rendering or IPC.
//! Args: NEW_OUTPUT_DIR SUBJECT CONSTRUCTION_FILE [CORRECTION | --pose POSE_PNG [CORRECTION]]
use aos_platform::{PlatformSubsystem, subsystem::PlatformConfig, illustration_image, illustration_service};
use aos_proto::{IllustGenerateImageRequest, IllustrationBrief, IllustrationImageStatus};
use std::{fs, path::PathBuf, time::{Duration, Instant}};
use base64::Engine;

fn critique(root: &std::path::Path, index: usize, subject: &str, png: &[u8])
    -> Result<(bool, Option<String>), Box<dyn std::error::Error>> {
    let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    let images = if index > 0 {
        vec![encode(&fs::read(root.join(format!("critique-{}-input.png", index - 1)))?), encode(png)]
    } else { vec![encode(png)] };
    let request = serde_json::json!({"model":"qwen3.5:9b","stream":false,"think":false,
        "format":"json","keep_alive":0,"options":{"temperature":0.1,"num_predict":2200,"num_ctx":12288},
        "messages":[{"role":"system","content":
            "Inspect the actual current illustration. Return JSON {regressed:boolean, regression_evidence:[string], needs_correction:boolean, visible_defects:[{region:string, observed_mark:string, desired_appearance:string}], correction:string}. With TWO images: image 1 is the source before the latest edit, image 2 is the candidate. Compare identity, pose, anatomy, clothing, props, composition and style. Mark regressed=true if the edit introduces a new defect or loses a correct feature. With ONE image regressed=false. Distinguish erroneous drawing marks from malformed anatomy: circular joint symbols, internal rods and guide lines should be erased and replaced with the visible skin/fur/fabric surface, not by adding joints or exposing a clothed limb. Every defect must identify a concrete visible feature and its location; vague judgments like 'incorrect anatomy' or 'fix the legs' are insufficient. Do not invent floating objects from ambiguous ground planes. Preserve correct clothes and natural species features. Write ONE short precise English edit instruction changing only the most important defective marks or shapes. Do not introduce unrequested details. If no supported correction exists, return needs_correction=false and an empty correction. If the candidate regressed, do not propose another edit on top of it. This is advisory, not proof of quality."},
            {"role":"user","content":format!("Original request: {subject}\nNumber of images: {}. Judge the LAST image as current. For every proposed mark removal, identify the actual visible surface and its color (for example gray trouser fabric, not 'skin or fabric'). The correction must preserve that exact material, coverage and color. If the surface is ambiguous, do not guess or propose its replacement. Never uncover a clothed limb to repair an anatomy guide.", images.len()),"images":images}]});
    // This is a local benchmark, separate from the application's provider routing.
    let response: serde_json::Value = reqwest::blocking::Client::builder().timeout(Duration::from_secs(300)).build()?
        .post("http://127.0.0.1:11434/api/chat").json(&request).send()?.error_for_status()?.json()?;
    fs::write(root.join(format!("critique-{index}-response.json")), serde_json::to_vec_pretty(&response)?)?;
    fs::write(root.join(format!("critique-{index}-input.png")), png)?;
    let judgment: serde_json::Value = serde_json::from_str(response["message"]["content"].as_str().ok_or("missing critique")?)?;
    fs::write(root.join(format!("critique-{index}.json")), serde_json::to_vec_pretty(&judgment)?)?;
    println!("Vision critique {index}: {judgment}");
    match judgment["regressed"].as_bool() {
        Some(true) if index == 0 => return Err("invalid critique: regression claimed without a source/candidate comparison; no export".into()),
        Some(true) => return Ok((true, None)),
        Some(false) => {},
        None => return Err("missing regression decision".into()),
    }
    match judgment["needs_correction"].as_bool() {
        Some(false) => Ok((false, None)),
        Some(true) => {
            let correction = judgment["correction"].as_str().filter(|s| !s.trim().is_empty()).ok_or("empty correction")?;
            Ok((false, Some(correction.to_owned())))
        },
        None => Err("invalid critique decision".into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    let _runtime_context = runtime.enter();
    let args: Vec<String> = std::env::args().collect();
    let pose_mode = (6..=7).contains(&args.len()) && args[4] == "--pose";
    if !(4..=5).contains(&args.len()) && !pose_mode { return Err("expected NEW_OUTPUT_DIR SUBJECT CONSTRUCTION_FILE [CORRECTION | --pose POSE_PNG [CORRECTION]]".into()); }
    let root = PathBuf::from(&args[1]);
    fs::create_dir(&root)?;
    let config: PlatformConfig = serde_json::from_value(serde_json::json!({
        "audit_dir":root.join("audit"),"storage_dir":root.join("storage"),
        "memory_dir":root.join("memory"),"modules_dir":root.join("modules"),
        "catalogue_file":root.join("missing-catalogue"),"community_catalogue_dir":root.join("community"),
        "skills_dir":root.join("skills"),"sessions_dir":root.join("sessions"),
        "secrets_file":root.join("secrets/keys.yaml"),"net_mode":"offline_strict"
    }))?;
    let sub = PlatformSubsystem::open(&config)?;
    let sid = {
        let sessions = sub.sessions.lock().unwrap();
        let meta = sessions.create(Some("Image service benchmark".into()), None).map_err(|e| e.to_string())?;
        sessions.illustration_lock_acquire(&meta.id, "human:benchmark", "service test", 600000).map_err(|e| e.to_string())?;
        sessions.illustration_set_brief(&meta.id, "human:benchmark", IllustrationBrief {
            subject: args[2].clone(), ..Default::default()
        }).map_err(|e| e.to_string())?;
        meta.id
    };
    let pose_reference_png = if pose_mode {
        let bytes = fs::read(&args[5])?;
        sub.fs.lock().unwrap().write_bytes("/downloads/benchmark-pose.png", &bytes, "human:benchmark",
            &["fs.write:/downloads/**".into()]).map_err(|e| e.to_string())?;
        Some("/downloads/benchmark-pose.png".into())
    } else { None };
    let started = illustration_image::start(sub.clone(), IllustGenerateImageRequest {
        session_id:sid.clone(), holder:"human:benchmark".into(), construction:fs::read_to_string(&args[3])?, frame_subject: None, seed: Some(42), pose_reference_png,
    })?;
    if pose_mode {
        assert!(started.image_run.as_ref().unwrap().pose_reference_png.as_deref()
            .is_some_and(|path| path.starts_with("/downloads/illustration/") && path.ends_with("-pose-reference.png")));
    }
    let start = Instant::now();
    let mut seen = 0;
    let mut refinements = 0;
    let auto_review = args.get(4).is_some_and(|s| s == "--auto-review");
    loop {
        let (_, doc) = sub.sessions.lock().unwrap().illustration_get(&sid).map_err(|e| e.to_string())?;
        if doc.pass_previews.len() != seen {
            seen = doc.pass_previews.len();
            println!("Published {seen} previews at {:.1}s, last_png={:?}", start.elapsed().as_secs_f32(), doc.last_png);
            fs::write(root.join(format!("publication-{seen}.json")), serde_json::to_vec_pretty(&doc)?)?;
        }
        let run = doc.image_run.as_ref().ok_or("missing image run")?;
        match run.status {
            IllustrationImageStatus::Running => {},
            IllustrationImageStatus::Failed => return Err(run.error.clone().unwrap_or_default().into()),
            IllustrationImageStatus::NeedsReview => {
                if pose_mode {
                    let path = run.pose_reference_png.as_deref().ok_or("missing archived pose metadata")?;
                    let (bytes, _, _) = sub.fs.lock().unwrap().read_bytes(path, &["fs.read:/downloads/**".into()])?;
                    assert_eq!(bytes, fs::read(&args[5])?, "archived pose differs from supplied input");
                    fs::write(root.join("pose-reference.png"), bytes)?;
                }
                if run.candidate_png.is_some() && run.candidate_selected.is_none() {
                    assert_eq!(doc.last_png, run.source_png, "unreviewed candidate replaced source");
                    let blocked = illustration_service::export_still(
                        &sub, &sid, "human:benchmark", None, None, None, "png");
                    assert!(blocked.is_err(), "unreviewed candidate allowed export");
                }
                let correction = if auto_review {
                    let path = run.candidate_png.as_deref().or(doc.last_png.as_deref()).ok_or("missing critique image")?;
                    let (bytes, _, _) = sub.fs.lock().unwrap().read_bytes(path, &["fs.read:/downloads/**".into()])?;
                    // reqwest blocking owns a runtime; keep it outside our entered
                    // platform runtime by running the independent critic on a thread.
                    let critique_root = root.clone();
                    let subject = args[2].clone();
                    let result = std::thread::spawn(move || critique(&critique_root, refinements, &subject, &bytes)
                        .map_err(|e| e.to_string())).join().map_err(|_| "critic panicked")??;
                    if run.candidate_png.is_some() {
                        let selected = sub.sessions.lock().unwrap().illustration_resolve_image(
                            &sid, "human:benchmark", &run.id, !result.0).map_err(|e| e.to_string())?;
                        fs::write(root.join(format!("selection-{refinements}.json")), serde_json::to_vec_pretty(&selected)?)?;
                        if result.0 {
                            assert_eq!(selected.last_png, run.source_png);
                            return Err("candidate regression detected; source restored by production store; no export".into());
                        }
                    }
                    if refinements < 2 { result.1 } else { None }
                } else {
                    if run.candidate_png.is_some() {
                        // Technical fixture selection, not an artistic approval.
                        sub.sessions.lock().unwrap().illustration_resolve_image(
                            &sid, "human:benchmark", &run.id, true).map_err(|e| e.to_string())?;
                    }
                    args.get(if pose_mode { 6 } else { 4 }).filter(|_| refinements == 0).cloned()
                };
                if let Some(correction) = correction {
                    if seen != 5 + refinements { return Err("missing initial passes".into()); }
                    fs::write(root.join(format!("correction-{refinements}.txt")), &correction)?;
                    let (source, _, _) = sub.fs.lock().unwrap().read_bytes(
                        doc.last_png.as_deref().ok_or("missing correction source")?, &["fs.read:/downloads/**".into()])?;
                    fs::write(root.join(format!("correction-{refinements}-source.png")), source)?;
                    let refined = illustration_image::refine(sub.clone(), aos_proto::IllustRefineImageRequest {
                        session_id: sid.clone(), holder: "human:benchmark".into(), correction: correction.clone(),
                    })?;
                    assert_eq!(refined.image_run.as_ref().unwrap().source_png, refined.last_png);
                    assert_eq!(refined.image_run.as_ref().unwrap().pose_reference_png, run.pose_reference_png);
                    assert_eq!(refined.pass_previews.len(), seen);
                    assert!(refined.revision > doc.revision);
                    refinements += 1;
                    println!("Targeted correction started; original preview and history retained.");
                    continue;
                }
                if seen != 5 + refinements { return Err("missing published passes".into()); }
                let review = illustration_service::review(&sub, &sid)?;
                assert_eq!(review["accepted"], false);
                let exported = illustration_service::export_still(&sub, &sid, "human:benchmark", None, None, None, "png")?;
                fs::write(root.join("export.json"), serde_json::to_vec_pretty(&exported)?)?;
                let path = exported["path"].as_str().ok_or("missing exported path")?;
                let (bytes, _, _) = sub.fs.lock().unwrap().read_bytes(path, &["fs.read:/downloads/**".into()])?;
                fs::write(root.join("final.png"), bytes)?;
                println!("PASS: {seen} published images, persisted state, real PNG export, review remains required.");
                break;
            }
        }
        if start.elapsed() > Duration::from_secs(600) { return Err("service benchmark timed out".into()); }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}
