//! Illustration bus helpers: export still / sheet / animate frames.

use crate::illustration_raster;
use crate::subsystem::PlatformSubsystem;
use aos_proto::{
    action_timeline, default_download_path, ease_io, illustration_digest, lerp_pose,
    normalize_download_path, resolve_timeline, review_illustration, sign_off_word,
    video_trace_error, DownloadKind, IllustAnimateResponse, IllustrationEngine,
    IllustrationPose, IllustrationRenderMode, IllustrationTimeline,
};
use std::process::Command;

fn stamp_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Illustration artefacts live under `/downloads/illustration`, even if a caller
/// still passes a canvas or video path.
fn illustration_download_path(path: &str) -> String {
    let path = normalize_download_path(path, DownloadKind::Illustration);
    for legacy in ["/downloads/canvas/", "/downloads/video/", "/downloads/images/"] {
        if let Some(rest) = path.strip_prefix(legacy) {
            return format!("/downloads/illustration/{rest}");
        }
    }
    path
}

pub(crate) fn write_download(
    s: &PlatformSubsystem,
    logical: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let caps = vec!["fs.write:/downloads/**".to_string()];
    let _version = s
        .fs
        .lock()
        .unwrap()
        .write_bytes(logical, bytes, "service:platformd", &caps)
        .map_err(|e| e.to_string())?;
    Ok(logical.to_string())
}

pub fn export_still(
    s: &PlatformSubsystem,
    session_id: &str,
    holder: &str,
    path: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    format: &str,
) -> Result<serde_json::Value, String> {
    let (meta, mut doc) = s
        .sessions
        .lock()
        .unwrap()
        .illustration_get(session_id)
        .map_err(|e| e.to_string())?;
    if let Some(lock) = doc.lock.as_ref() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        if lock.expires_ms > now && lock.holder != holder && !holder.starts_with("human") {
            return Err(format!("illustration verrouillé par {}", lock.holder));
        }
    }
    if let Some(run) = &doc.image_run {
        if run.candidate_png.is_some() && run.candidate_selected.is_none() {
            return Err("comparer la retouche à sa source puis choisir ou rejeter via illust.resolve_image avant export".into());
        }
        if run.status != aos_proto::IllustrationImageStatus::NeedsReview {
            return Err("rendu image incomplet : attendre la fin ou corriger l'erreur".into());
        }
        if format != "png" { return Err("le moteur image exporte un PNG, pas de géométrie vectorielle".into()); }
        let source = doc.last_png.as_deref().ok_or("PNG final absent")?;
        let (bytes, _, _) = s.fs.lock().unwrap().read_bytes(source, &["fs.read:/downloads/**".into()])
            .map_err(|e| e.to_string())?;
        let path = illustration_download_path(&path.unwrap_or_else(|| default_download_path(
            DownloadKind::Illustration, &format!("illust-{}-{}.png", meta.id, stamp_ms()))));
        write_download(s, &path, &bytes)?;
        if holder.starts_with("agent") {
            let _ = s.sessions.lock().unwrap().illustration_lock_release(session_id, holder);
        }
        return Ok(serde_json::json!({"path":path,"format":"png","visual_review":"required",
            "note":"rendu natif 768px; export ne vaut pas validation artistique"}));
    }
    // Agents often skip compose or emit unreadable LLM snowmen. Re-run puppet
    // enrich before raster so known subjects (cat+cushion, …) get the recipe.
    if !doc.brief.subject.trim().is_empty() {
        doc = s
            .sessions
            .lock()
            .unwrap()
            .illustration_ensure_composed(session_id)
            .map_err(|e| e.to_string())?;
    }
    // Skill loop: contact sheet + structural review before agent export.
    if holder.starts_with("agent") {
        let review = aos_proto::review_illustration(&doc);
        let blocking: Vec<&str> = review
            .issues
            .iter()
            .filter(|i| i.severity == "error" && i.kind != "missing_contact_sheet")
            .map(|i| i.kind.as_str())
            .collect();
        if !blocking.is_empty() {
            return Err(format!(
                "export refusé (score structurel={:.2}): {} — corriger la construction avec illust.compose, fournir les parts du rendu final et construction_phase=final, puis illust.render_sheet + illust.review",
                review.score,
                blocking.join(", ")
            ));
        }
        if doc
            .last_sheet_png
            .as_ref()
            .map(|p| p.trim().is_empty())
            .unwrap_or(true)
        {
            // Auto-build contact sheet once, then require the agent to review next time
            // if they skipped the skill step entirely.
            let _ = render_sheet(s, session_id, Some(240))?;
            doc = s
                .sessions
                .lock()
                .unwrap()
                .illustration_get(session_id)
                .map(|( _m, d)| d)
                .map_err(|e| e.to_string())?;
        }
    }
    let w = width.unwrap_or(1024);
    let h = height.unwrap_or(1024);
    let stamp = stamp_ms();
    if format == "json" {
        let bytes = illustration_raster::export_sidecar_json(&doc)?;
        let path = path.unwrap_or_else(|| {
            default_download_path(
                DownloadKind::Illustration,
                &format!("illust-{}-{}.json", meta.id, stamp),
            )
        });
        let path = illustration_download_path(&path);
        write_download(s, &path, &bytes)?;
        return Ok(serde_json::json!({ "path": path, "format": "json" }));
    }
    let bytes = illustration_raster::export_png(&doc, w, h)?;
    let path = path.unwrap_or_else(|| {
        default_download_path(
            DownloadKind::Illustration,
            &format!("illust-{}-{}.png", meta.id, stamp),
        )
    });
    let path = illustration_download_path(&path);
    write_download(s, &path, &bytes)?;
    let doc = s
        .sessions
        .lock()
        .unwrap()
        .illustration_set_paths(session_id, Some(path.clone()), None, None)
        .map_err(|e| e.to_string())?;
    if holder.starts_with("agent") {
        let _ = s
            .sessions
            .lock()
            .unwrap()
            .illustration_lock_release(session_id, holder);
    }
    Ok(serde_json::json!({
        "path": path,
        "format": "png",
        "digest": illustration_digest(&doc),
        "illustration_open": meta.illustration_open,
    }))
}

/// Write a still preview without releasing the illustration lock (compose refresh).
pub fn export_preview(
    s: &PlatformSubsystem,
    session_id: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<String, String> {
    let (meta, doc) = s
        .sessions
        .lock()
        .unwrap()
        .illustration_get(session_id)
        .map_err(|e| e.to_string())?;
    let w = width.unwrap_or(720);
    let h = height.unwrap_or(720);
    let stamp = stamp_ms();
    let bytes = illustration_raster::export_png(&doc, w, h)?;
    let path = default_download_path(
        DownloadKind::Illustration,
        &format!("illust-preview-{}-{}.png", meta.id, stamp),
    );
    let path = illustration_download_path(&path);
    write_download(s, &path, &bytes)?;
    let _ = s
        .sessions
        .lock()
        .unwrap()
        .illustration_publish_pass(
            session_id,
            doc.spec.as_ref().map(|spec| spec.construction_phase).unwrap_or_default(),
            doc.revision,
            path.clone(),
        )
        .map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn render_sheet(
    s: &PlatformSubsystem,
    session_id: &str,
    width: Option<u32>,
) -> Result<serde_json::Value, String> {
    let (meta, doc) = s
        .sessions
        .lock()
        .unwrap()
        .illustration_get(session_id)
        .map_err(|e| e.to_string())?;
    if doc.image_run.is_some() { return Err("rendu image : consulter les pass_previews, pas la planche vectorielle".into()); }
    let cell = width.unwrap_or(240);
    let bytes = illustration_raster::export_sheet_png(&doc, cell)?;
    let model_bytes = illustration_raster::export_model_sheet_png(&doc, cell)?;
    let stamp = stamp_ms();
    let path = default_download_path(
        DownloadKind::Illustration,
        &format!("illust-sheet-{}-{}.png", meta.id, stamp),
    );
    let path = illustration_download_path(&path);
    write_download(s, &path, &bytes)?;
    let model_path = illustration_download_path(&default_download_path(
        DownloadKind::Illustration,
        &format!("illust-model-sheet-{}-{}.png", meta.id, stamp),
    ));
    write_download(s, &model_path, &model_bytes)?;
    let _ = s
        .sessions
        .lock()
        .unwrap()
        .illustration_set_paths(session_id, None, Some(path.clone()), None)
        .map_err(|e| e.to_string())?;
    let doc = s
        .sessions
        .lock()
        .unwrap()
        .illustration_set_model_sheet_path(session_id, model_path.clone())
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "path": path,
        "model_sheet_path": model_path,
        "digest": illustration_digest(&doc),
    }))
}

pub fn review(s: &PlatformSubsystem, session_id: &str) -> Result<serde_json::Value, String> {
    let (_, doc) = s
        .sessions
        .lock()
        .unwrap()
        .illustration_get(session_id)
        .map_err(|e| e.to_string())?;
    if let Some(run) = &doc.image_run {
        return Ok(serde_json::json!({"image_run":run,"path":doc.last_png,
            "visual_review":"required","accepted":false,
            "note":"les scores structurels vectoriels ne valident pas une image générée"}));
    }
    let report = review_illustration(&doc);
    serde_json::to_value(report).map_err(|e| e.to_string())
}

fn find_ffmpeg() -> Option<String> {
    if let Ok(p) = std::env::var("FFMPEG") {
        if std::path::Path::new(&p).exists() || Command::new(&p).arg("-version").output().is_ok() {
            return Some(p);
        }
    }
    for name in ["ffmpeg", "ffmpeg.exe"] {
        if Command::new(name).arg("-version").output().is_ok() {
            return Some(name.to_string());
        }
    }
    None
}

pub fn animate(
    s: &PlatformSubsystem,
    session_id: &str,
    holder: &str,
    timeline: Option<IllustrationTimeline>,
    duration_s: Option<f32>,
    width: Option<u32>,
    path: Option<String>,
) -> Result<IllustAnimateResponse, String> {
    let (_, mut doc) = s
        .sessions
        .lock()
        .unwrap()
        .illustration_get(session_id)
        .map_err(|e| e.to_string())?;
    if doc.spec.is_none() {
        return Err("compose une scène avant illust.animate".into());
    }
    if let Some(msg) = video_trace_error(&doc.brief) {
        return Err(msg.into());
    }
    aos_proto::apply_prompt_defaults(&mut doc.brief);
    if let Some(tl) = timeline {
        doc = s
            .sessions
            .lock()
            .unwrap()
            .illustration_set_timeline(session_id, holder, tl)
            .map_err(|e| e.to_string())?;
    }
    if let Some(secs) = duration_s {
        doc.timeline.beats = resolve_timeline(&doc.timeline.beats, secs, &doc.brief);
        doc = s
            .sessions
            .lock()
            .unwrap()
            .illustration_set_timeline(session_id, holder, doc.timeline.clone())
            .map_err(|e| e.to_string())?;
    } else if doc.timeline.beats.is_empty() {
        doc.timeline.beats = action_timeline(&doc.brief, 4.0);
        doc = s
            .sessions
            .lock()
            .unwrap()
            .illustration_set_timeline(session_id, holder, doc.timeline.clone())
            .map_err(|e| e.to_string())?;
    }

    let w = width.unwrap_or(720);
    let h = w;
    // Use a real 24 fps drawing timebase. The previous 12 fps sequence was
    // only duplicated by ffmpeg, which could not improve motion quality.
    let fps_draw = 24.0f32;
    let stamp = stamp_ms();
    let frames_logical = default_download_path(
        DownloadKind::Illustration,
        &format!("illust-frames-{}-{}", session_id, stamp),
    );
    let frames_logical = illustration_download_path(&frames_logical);

    let host_frames = s
        .fs
        .lock()
        .unwrap()
        .resolve_host(&frames_logical)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&host_frames).map_err(|e| e.to_string())?;

    let mut frame_i = 0u32;
    let base_cam = doc
        .spec
        .as_ref()
        .map(|sp| sp.camera.clone())
        .unwrap_or_default();
    let base_mode = doc
        .spec
        .as_ref()
        .map(|sp| sp.mode)
        .unwrap_or(IllustrationRenderMode::Normal);

    let frames_total: u32 = doc
        .timeline
        .beats
        .iter()
        .map(|b| (b.dur_s * fps_draw).round().max(1.0) as u32)
        .sum::<u32>()
        .max(1);
    resolve_photo_host(s, &mut doc);
    let engine = doc.brief.engine;
    let sign_word = sign_off_word(&doc.brief);
    let beats = doc.timeline.beats.clone();
    let mut prev_pose = IllustrationPose::default();

    for (bi, beat) in beats.iter().enumerate() {
        let n = (beat.dur_s * fps_draw).round().max(1.0) as u32;
        let key_drawing = doc
            .spec
            .as_ref()
            .and_then(|spec| spec.key_drawings.iter().find(|drawing| drawing.id == beat.name))
            .cloned();
        let mut cam = beat.camera.clone().unwrap_or_else(|| base_cam.clone());
        if bi == 0 {
            cam.zoom = (cam.zoom * 1.08).clamp(0.3, 2.5);
        }
        let mode = beat.mode.unwrap_or(base_mode);
        let look_changed = bi > 0 && beat.mode.is_some() && beats[bi - 1].mode != beat.mode;
        let sign = beat.name == "signoff";
        for k in 0..n {
            let local = (k as f32 + 1.0) / n as f32;
            let pose = lerp_pose(&prev_pose, &beat.pose, ease_io(local));
            let png = match engine {
                IllustrationEngine::Sand => {
                    let t = frame_i as f32 / frames_total as f32;
                    crate::illustration_sand::render_sand_png(w, h, t)?
                }
                IllustrationEngine::Paper => {
                    let t = frame_i as f32 / frames_total as f32;
                    crate::illustration_paper::render_paper_png(w, h, t)?
                }
                IllustrationEngine::Flat | IllustrationEngine::Found => {
                    let mut frame_doc = doc.clone();
                    let mut fx = illustration_raster::IllustrationFrameFx {
                        speed: pose.walk.abs(),
                        preserve_drawing: key_drawing.is_some(),
                        ..Default::default()
                    };
                    if let (Some(spec), Some(drawing)) = (frame_doc.spec.as_mut(), key_drawing.as_ref()) {
                        // A key drawing is a complete cel. Keep the base brief,
                        // camera and timeline, but replace every visible part.
                        spec.parts = drawing.parts.clone();
                        spec.pose = drawing.pose.clone();
                    }
                    if bi == 0 && !sign {
                        fx.progress = Some(local);
                    }
                    if look_changed {
                        let iris_n = n.min(12);
                        if k < iris_n {
                            fx.iris = Some((k as f32 + 1.0) / iris_n as f32);
                        }
                    }
                    if sign {
                        fx.sign_off = Some(sign_word.clone());
                        fx.sign_progress = (local / 0.65).min(1.0);
                    }
                    illustration_raster::export_frame_fx(
                        &frame_doc,
                        &pose,
                        &cam,
                        mode,
                        w,
                        h,
                        &fx,
                    )?
                }
            };
            let name = format!("{:04}.png", frame_i);
            std::fs::write(host_frames.join(&name), &png).map_err(|e| e.to_string())?;
            frame_i += 1;
        }
        prev_pose = beat.pose.clone();
    }

    let contact_logical = default_download_path(
        DownloadKind::Illustration,
        &format!("illust-contact-{}-{}.png", session_id, stamp),
    );
    let contact_logical = illustration_download_path(&contact_logical);
    let contact_bytes = build_contact_sheet(&host_frames, frame_i, 6)?;
    write_download(s, &contact_logical, &contact_bytes)?;

    let mut mp4_path = None;
    let mut message = format!("{frame_i} drawn frames @ 24fps in {frames_logical}");
    if let Some(ff) = find_ffmpeg() {
        let out_logical = path.unwrap_or_else(|| {
            default_download_path(
                DownloadKind::Illustration,
                &format!("illust-{}-{}.mp4", session_id, stamp),
            )
        });
        let out_logical = illustration_download_path(&out_logical);
        let host_mp4 = s
            .fs
            .lock()
            .unwrap()
            .resolve_host(&out_logical)
            .map_err(|e| e.to_string())?;
        if let Some(parent) = host_mp4.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let pattern = host_frames.join("%04d.png");
        let status = Command::new(&ff)
            .args([
                "-y",
                "-framerate",
                "24",
                "-i",
                &pattern.to_string_lossy(),
                "-r",
                "24",
                "-pix_fmt",
                "yuv420p",
                "-crf",
                "18",
                &host_mp4.to_string_lossy(),
            ])
            .status();
        match status {
            Ok(st) if st.success() => {
                let bytes = std::fs::read(&host_mp4).unwrap_or_default();
                let _ = write_download(s, &out_logical, &bytes);
                mp4_path = Some(out_logical.clone());
                message.push_str(&format!("; mp4 {out_logical}"));
                if let Some(scored) = mux_score(s, &ff, &host_mp4, &out_logical, &doc.timeline.beats)
                {
                    mp4_path = Some(scored);
                    message.push_str(" + score");
                }
                let _ = s.sessions.lock().unwrap().illustration_set_paths(
                    session_id,
                    None,
                    None,
                    mp4_path.clone(),
                );
            }
            Ok(st) => message.push_str(&format!("; ffmpeg exit {st}")),
            Err(e) => message.push_str(&format!("; ffmpeg err {e}")),
        }
    } else {
        message.push_str("; ffmpeg absent — frames only (set FFMPEG=)");
    }

    let (_, doc) = s
        .sessions
        .lock()
        .unwrap()
        .illustration_get(session_id)
        .map_err(|e| e.to_string())?;

    Ok(IllustAnimateResponse {
        doc,
        frames_dir: frames_logical,
        mp4_path,
        contact_sheet: Some(contact_logical),
        drawn_frames: frame_i,
        message,
    })
}

fn resolve_photo_host(s: &PlatformSubsystem, doc: &mut aos_proto::IllustrationDoc) {
    let path = doc.brief.photo.trim().to_string();
    if path.is_empty() || std::path::Path::new(&path).exists() {
        return;
    }
    let Ok(host) = s.fs.lock().unwrap().resolve_host(&path) else {
        return;
    };
    if host.exists() {
        doc.brief.photo = host.to_string_lossy().to_string();
    }
}

fn mux_score(
    s: &PlatformSubsystem,
    ffmpeg: &str,
    host_mp4: &std::path::Path,
    mp4_logical: &str,
    beats: &[aos_proto::IllustrationTimelineBeat],
) -> Option<String> {
    let wav = crate::illustration_score::score_wav(beats);
    let wav_logical = mp4_logical.trim_end_matches(".mp4").to_string() + ".wav";
    let wav_logical = if wav_logical.ends_with(".wav") {
        wav_logical
    } else {
        format!("{mp4_logical}.wav")
    };
    let host_wav = s.fs.lock().unwrap().resolve_host(&wav_logical).ok()?;
    if let Some(parent) = host_wav.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&host_wav, &wav).ok()?;
    let _ = write_download(s, &wav_logical, &wav);
    let muxed = host_mp4.with_extension("scored.mp4");
    let status = Command::new(ffmpeg)
        .args([
            "-y",
            "-i",
            &host_mp4.to_string_lossy(),
            "-i",
            &host_wav.to_string_lossy(),
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-shortest",
            &muxed.to_string_lossy(),
        ])
        .status()
        .ok()?;
    if !status.success() {
        return Some(mp4_logical.to_string());
    }
    let bytes = std::fs::read(&muxed).ok()?;
    let final_logical = mp4_logical.trim_end_matches(".mp4").to_string() + "-final.mp4";
    write_download(s, &final_logical, &bytes).ok()?;
    Some(final_logical)
}

fn build_contact_sheet(
    frames_dir: &std::path::Path,
    total: u32,
    step: u32,
) -> Result<Vec<u8>, String> {
    use image::{GenericImage, ImageBuffer, RgbImage};
    let step = step.max(1);
    let indices: Vec<u32> = (0..total).filter(|i| i % step == 0).collect();
    if indices.is_empty() {
        return Err("no frames for contact sheet".into());
    }
    let first = image::open(frames_dir.join(format!("{:04}.png", indices[0])))
        .map_err(|e| e.to_string())?
        .to_rgb8();
    let tw = 240u32;
    let th = ((first.height() as f32) * (tw as f32 / first.width() as f32)).round() as u32;
    let cols = 6u32;
    let rows = ((indices.len() as u32) + cols - 1) / cols;
    let mut sheet: RgbImage =
        ImageBuffer::from_pixel(cols * tw, rows * th.max(1), image::Rgb([243, 230, 207]));
    for (n, idx) in indices.iter().enumerate() {
        let img = image::open(frames_dir.join(format!("{:04}.png", idx)))
            .map_err(|e| e.to_string())?
            .to_rgb8();
        let resized = image::imageops::resize(&img, tw, th.max(1), image::imageops::FilterType::Triangle);
        let col = (n as u32) % cols;
        let row = (n as u32) / cols;
        sheet
            .copy_from(&resized, col * tw, row * th)
            .map_err(|e| e.to_string())?;
    }
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    sheet
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use aos_proto::{resolve_timeline, IllustrationBrief, IllustrationTimelineBeat};

    #[test]
    fn duration_builds_and_scales() {
        let built = resolve_timeline(&[], 4.0, &IllustrationBrief::default());
        let sum: f32 = built.iter().map(|b| b.dur_s).sum();
        assert!((sum - 4.0).abs() < 0.08, "{sum}");
        assert!(built.len() >= 2);
        assert!(built.iter().any(|b| b.name == "signoff"));

        let scaled = resolve_timeline(
            &[
                IllustrationTimelineBeat {
                    name: "a".into(),
                    dur_s: 1.0,
                    pose: Default::default(),
                    camera: None,
                    mode: None,
                },
                IllustrationTimelineBeat {
                    name: "b".into(),
                    dur_s: 1.0,
                    pose: Default::default(),
                    camera: None,
                    mode: None,
                },
            ],
            6.0,
            &IllustrationBrief::default(),
        );
        let sum: f32 = scaled.iter().map(|b| b.dur_s).sum();
        assert!((sum - 6.0).abs() < 0.05, "{sum}");
        assert_eq!(scaled[0].name, "a");
    }
}
