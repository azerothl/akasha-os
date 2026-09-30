//! Background image-editing jobs. Publish completed passes to the same document
//! polled by the Illustration panel. Successful generation still requires review.
use crate::{illustration_service::write_download, PlatformSubsystem};
use aos_proto::{IllustrationConstructionPhase as Phase, IllustrationImageStatus as Status};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

static BUSY: AtomicBool = AtomicBool::new(false);
fn validate_pose_path(path: &str) -> Result<(), String> {
    if !path.starts_with("/downloads/") || path.len() > 2048 || path.contains(['\\', ':', '\0'])
        || path.split('/').skip(1).any(|part| part.is_empty() || part == "." || part == "..") {
        return Err("pose_reference_png doit être un chemin absolu normalisé dans /downloads".into());
    }
    Ok(())
}

fn validate_pose_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > 16 * 1024 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("guide de pose : PNG requis, maximum 16 Mio".into());
    }
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png)
        .into_dimensions().map_err(|e| format!("guide de pose invalide : {e}"))?;
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return Err("guide de pose : dimensions entre 1 et 2048 pixels".into());
    }
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|e| format!("guide de pose invalide : {e}"))?;
    Ok(())
}
fn refinement_prompt(subject: &str, correction: &str) -> String {
    format!("Edit the reference illustration. Requested local correction: {correction}\n\nOriginal scene and style: {subject}\n\nChange only the specified visible defect. Preserve the existing identity, species, pose, composition, contacts, props and style everywhere else. Preserve the exact existing garment coverage, fabric and colors: removing a construction mark from clothing must restore that same fabric, never expose skin or recolor the limb. On an animal restore the existing fur, feather or scale surface. Only change clothing or colors if the requested correction explicitly asks for that change. Do not add construction symbols or extra limbs. Add or remove an object or effect only when explicitly requested by the correction; otherwise preserve the existing scene. Return one finished illustration, not a comparison sheet.")
}
struct Permit;
impl Drop for Permit { fn drop(&mut self) { BUSY.store(false, Ordering::SeqCst); } }

pub fn start(s: Arc<PlatformSubsystem>, req: aos_proto::IllustGenerateImageRequest)
    -> Result<aos_proto::IllustrationDoc, String> {
    run(s, req, false)
}

pub fn refine(s: Arc<PlatformSubsystem>, req: aos_proto::IllustRefineImageRequest)
    -> Result<aos_proto::IllustrationDoc, String> {
    run(s, aos_proto::IllustGenerateImageRequest {
        session_id: req.session_id, holder: req.holder, construction: req.correction, frame_subject: None, seed: None, pose_reference_png: None,
    }, true)
}

fn run(s: Arc<PlatformSubsystem>, req: aos_proto::IllustGenerateImageRequest, editing: bool)
    -> Result<aos_proto::IllustrationDoc, String> {
    if req.construction.trim().is_empty() || req.construction.len() > 12000 {
        return Err("construction requis (pose et composition, maximum 12000 octets)".into());
    }
    if req.frame_subject.as_ref().is_some_and(|s| s.trim().is_empty() || s.len() > 12000) {
        return Err("frame_subject doit décrire une image-clé non vide (maximum 12000 octets)".into());
    }
    let pose_bytes = match req.pose_reference_png.as_deref() {
        Some(path) => {
            validate_pose_path(path)?;
            let bytes = s.fs.lock().unwrap().read_bytes(path, &["fs.read:/downloads/**".into()])
                .map_err(|e| e.to_string())?.0;
            validate_pose_png(&bytes)?;
            Some(bytes)
        }
        None => None,
    };
    if !aos_sd::image_engine_available() {
        return Err("moteur d'édition absent : configurer AOS_SD_BIN (sd-cli)".into());
    }
    let model_dir = std::path::PathBuf::from(std::env::var("AOS_ILLUSTRATION_MODEL_DIR")
        .map_err(|_| "configurer AOS_ILLUSTRATION_MODEL_DIR avec les modèles Klein 4B, Qwen3 4B et VAE")?);
    let weights = model_dir.join("flux-2-klein-4b-Q8_0.gguf");
    let mut opts = aos_sd::ImageGenOpts {
        width: 768, height: 768, steps: 4, cfg_scale: Some(1.0), seed: None,
        sampling_method: Some("euler".into()), diffusion_model: Some(weights.clone()),
        llm_path: Some(model_dir.join("Qwen3-4B-Q4_K_M.gguf")),
        vae_path: Some(model_dir.join("split_files/vae/flux2-vae.safetensors")),
        offload_to_cpu: true, diffusion_fa: true, ..Default::default()
    };
    for path in [&weights, opts.llm_path.as_ref().unwrap(), opts.vae_path.as_ref().unwrap()] {
        if !path.is_file() { return Err(format!("modèle absent : {}", path.display())); }
    }
    if BUSY.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("une génération Illustration est déjà en cours".into());
    }
    let permit = Permit;
    let run_id = format!("{:016x}-{:016x}", rand::random::<u64>(), rand::random::<u64>());
    let archived_pose = pose_bytes.as_ref().map(|_| format!("/downloads/illustration/{run_id}-pose-reference.png"));
    let (doc, reference) = {
        let sessions = s.sessions.lock().unwrap();
        let reference = if editing {
            let (_, current) = sessions.illustration_get(&req.session_id).map_err(|e| e.to_string())?;
            if !current.image_run.as_ref().is_some_and(|r| r.status == Status::NeedsReview) {
                return Err("attendre un rendu image terminé avant de le retoucher".into());
            }
            if current.image_run.as_ref().is_some_and(|r| r.candidate_png.is_some() && r.candidate_selected.is_none()) {
                return Err("choisir ou rejeter la retouche proposée avant d'en lancer une autre".into());
            }
            let path = current.last_png.as_deref().ok_or("image à retoucher absente")?;
            Some(s.fs.lock().unwrap().read_bytes(path, &["fs.read:/downloads/**".into()])
                .map_err(|e| e.to_string())?.0)
        } else { None };
        let doc = sessions.illustration_begin_image_run(
            &req.session_id, &req.holder, run_id.clone(), editing, req.frame_subject.clone(),
            req.seed.unwrap_or_else(rand::random), archived_pose.clone()).map_err(|e| e.to_string())?;
        (doc, reference)
    };
    let revision = doc.revision;
    opts.seed = doc.image_run.as_ref().and_then(|r| r.seed).map(i64::from);
    let frame_subject = doc.image_run.as_ref().and_then(|r| r.frame_subject.as_deref())
        .unwrap_or(&doc.brief.subject);
    let subject = format!("{}\nRequested illustration style: {:?}", frame_subject, doc.brief.look);
    let sid = req.session_id.clone();
    let thread_s = s.clone();
    let thread_run_id = run_id.clone();
    let spawn_result = std::thread::Builder::new().name("illustration-image".into()).spawn(move || {
        let _permit = permit;
        let run_id = thread_run_id;
        let output = std::env::temp_dir().join(format!("akasha-illustration-{run_id}"));
        let mut phase = if editing { Phase::Final } else { Phase::Skeleton };
        let mut publish = |pass, host_path: &std::path::Path| {
                phase = match pass {
                    aos_sd::illustration::DrawingPass::Skeleton => Phase::Skeleton,
                    aos_sd::illustration::DrawingPass::Volumes => Phase::Volumes,
                    aos_sd::illustration::DrawingPass::Contours => Phase::Contours,
                    aos_sd::illustration::DrawingPass::Details => Phase::Details,
                    aos_sd::illustration::DrawingPass::Final => Phase::Final,
                };
                let logical = format!("/downloads/illustration/{run_id}-{}.png", pass.name());
                let bytes = std::fs::read(host_path)?;
                let fail = |detail: String| aos_sd::MediaError::EngineFailed { engine: "illustration".into(), detail };
                write_download(&thread_s, &logical, &bytes).map_err(fail)?;
                thread_s.sessions.lock().unwrap().illustration_update_image_run(
                    &sid, &run_id, revision, phase, Some(logical), Status::Running, None)
                    .map_err(|e| fail(e.to_string()))?;
                Ok(())
            };
        let result: Result<(), aos_sd::MediaError> = if let Some(reference) = reference {
            (|| {
                std::fs::create_dir(&output)?;
                let source = output.join("source.png");
                let dest = output.join("refined.png");
                std::fs::write(&source, reference)?;
                let mut opts = opts.clone();
                opts.reference_image_paths = vec![source];
                let prompt = refinement_prompt(&subject, &req.construction);
                std::fs::write(output.join("correction.txt"), &prompt)?;
                aos_sd::generate_image_strict_progress(&weights, &prompt, &dest, &opts, |_, _| {})?;
                image::open(&dest).map_err(|e| aos_sd::MediaError::EngineFailed {
                    engine:"illustration".into(), detail:e.to_string() })?;
                publish(aos_sd::illustration::DrawingPass::Final, &dest)
            })()
        } else if let Some(bytes) = pose_bytes {
            (|| {
                let logical = archived_pose.as_deref().expect("pose archive assigned with bytes");
                write_download(&thread_s, logical, &bytes).map_err(|detail| aos_sd::MediaError::EngineFailed {
                    engine: "illustration".into(), detail,
                })?;
                // Keep the bootstrap beside the output directory: the pass
                // runner requires that directory not exist before it starts.
                let guide_dir = output.with_extension("guide");
                std::fs::create_dir(&guide_dir)?;
                let pose = guide_dir.join("pose.png");
                std::fs::write(&pose, bytes)?;
                aos_sd::illustration::generate_pose_guided_passes(
                    &weights, &subject, &req.construction, &pose, &output, &opts, &mut publish).map(|_| ())
            })()
        } else {
            aos_sd::illustration::generate_planned_passes(
                &weights, &subject, &req.construction, &output, &opts, &mut publish).map(|_| ())
        };
        let (status, error) = match result {
            Ok(_) => (Status::NeedsReview, None),
            Err(e) => (Status::Failed, Some(e.to_string())),
        };
        let _ = thread_s.sessions.lock().unwrap().illustration_update_image_run(
            &sid, &run_id, revision, phase, None, status, error);
    });
    if let Err(error) = spawn_result {
        let _ = s.sessions.lock().unwrap().illustration_update_image_run(
            &req.session_id, &run_id, revision, Phase::Skeleton, None, Status::Failed, Some(error.to_string()));
        return Err(error.to_string());
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::{refinement_prompt, validate_pose_path, validate_pose_png};

    #[test]
    fn pose_guide_rejects_host_paths_traversal_and_invalid_images() {
        assert!(validate_pose_path("/downloads/illustration/pose.png").is_ok());
        for path in ["C:/pose.png", "/downloads/../secrets/key", "/downloads//pose.png", "/downloads/./pose.png", "/downloads/pose:stream", "/downloads\\pose.png", "https://example.com/pose.png"] {
            assert!(validate_pose_path(path).is_err(), "{path}");
        }
        let png = |width| {
            let mut out = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(width, 8).write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        assert!(validate_pose_png(&png(8)).is_ok());
        assert!(validate_pose_png(&png(2049)).is_err());
        assert!(validate_pose_png(b"not a PNG").is_err());
        let good = png(8);
        assert!(validate_pose_png(&good[..good.len() / 2]).is_err());
    }

    #[test]
    fn seed_request_is_optional_and_bounded() {
        let mut value = serde_json::json!({"session_id":"test","holder":"test","construction":"pose"});
        let legacy: aos_proto::IllustGenerateImageRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(legacy.seed, None);
        assert_eq!(legacy.pose_reference_png, None);
        value["seed"] = serde_json::json!(u32::MAX);
        let explicit: aos_proto::IllustGenerateImageRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(explicit.seed, Some(u32::MAX));
        value["seed"] = serde_json::json!(-1);
        assert!(serde_json::from_value::<aos_proto::IllustGenerateImageRequest>(value).is_err());
    }

    #[test]
    fn local_edit_carries_scene_and_protects_existing_surfaces() {
        let prompt = refinement_prompt("a fox asleep on a blue cushion; pencil", "remove the circular guide on the hind leg");
        assert!(prompt.contains("a fox asleep on a blue cushion; pencil"));
        assert!(prompt.contains("remove the circular guide on the hind leg"));
        assert!(prompt.contains("restore that same fabric, never expose skin"));
        assert!(prompt.contains("existing fur, feather or scale surface"));
        assert!(!prompt.contains("gardener"));
    }

    #[test]
    fn local_edit_can_restore_an_explicitly_requested_effect() {
        let prompt = refinement_prompt("a smoking pipe", "add a thin wisp of smoke rising from the bowl");
        assert!(prompt.contains("add a thin wisp of smoke rising from the bowl"));
        assert!(prompt.contains("only when explicitly requested by the correction"));
        assert!(!prompt.contains("Do not add construction symbols, limbs, objects or smoke"));
    }
}
