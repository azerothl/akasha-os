//! Illustration bus helpers: export still / sheet / animate frames.

use crate::illustration_raster;
use crate::subsystem::PlatformSubsystem;
use aos_proto::{
    default_download_path, illustration_digest, normalize_download_path, review_illustration,
    DownloadKind, IllustAnimateResponse, IllustrationPose, IllustrationRenderMode,
    IllustrationTimeline,
};
use std::process::Command;

fn stamp_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn write_download(
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
    let (meta, doc) = s
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
    let w = width.unwrap_or(1024);
    let h = height.unwrap_or(1024);
    let stamp = stamp_ms();
    if format == "json" {
        let bytes = illustration_raster::export_sidecar_json(&doc)?;
        let path = path.unwrap_or_else(|| {
            default_download_path(
                DownloadKind::Canvas,
                &format!("illust-{}-{}.json", meta.id, stamp),
            )
        });
        let path = normalize_download_path(&path, DownloadKind::Canvas);
        write_download(s, &path, &bytes)?;
        return Ok(serde_json::json!({ "path": path, "format": "json" }));
    }
    let bytes = illustration_raster::export_png(&doc, w, h)?;
    let path = path.unwrap_or_else(|| {
        default_download_path(
            DownloadKind::Canvas,
            &format!("illust-{}-{}.png", meta.id, stamp),
        )
    });
    let path = normalize_download_path(&path, DownloadKind::Canvas);
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
        DownloadKind::Canvas,
        &format!("illust-preview-{}-{}.png", meta.id, stamp),
    );
    let path = normalize_download_path(&path, DownloadKind::Canvas);
    write_download(s, &path, &bytes)?;
    let _ = s
        .sessions
        .lock()
        .unwrap()
        .illustration_set_paths(session_id, Some(path.clone()), None, None);
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
    let cell = width.unwrap_or(240);
    let bytes = illustration_raster::export_sheet_png(&doc, cell)?;
    let stamp = stamp_ms();
    let path = default_download_path(
        DownloadKind::Canvas,
        &format!("illust-sheet-{}-{}.png", meta.id, stamp),
    );
    let path = normalize_download_path(&path, DownloadKind::Canvas);
    write_download(s, &path, &bytes)?;
    let doc = s
        .sessions
        .lock()
        .unwrap()
        .illustration_set_paths(session_id, None, Some(path.clone()), None)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "path": path,
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
    if let Some(tl) = timeline {
        doc = s
            .sessions
            .lock()
            .unwrap()
            .illustration_set_timeline(session_id, holder, tl)
            .map_err(|e| e.to_string())?;
    }
    if doc.timeline.beats.is_empty() {
        doc.timeline.beats = vec![
            aos_proto::IllustrationTimelineBeat {
                name: "hold".into(),
                dur_s: 1.5,
                pose: IllustrationPose::default(),
                camera: None,
                mode: None,
            },
            aos_proto::IllustrationTimelineBeat {
                name: "twitch".into(),
                dur_s: 1.0,
                pose: IllustrationPose {
                    twitch: 1.0,
                    tilt: 0.3,
                    ..Default::default()
                },
                camera: None,
                mode: None,
            },
        ];
        doc = s
            .sessions
            .lock()
            .unwrap()
            .illustration_set_timeline(session_id, holder, doc.timeline.clone())
            .map_err(|e| e.to_string())?;
    }

    let w = width.unwrap_or(720);
    let h = w;
    let fps_draw = 12.0f32;
    let stamp = stamp_ms();
    let frames_logical = default_download_path(
        DownloadKind::Video,
        &format!("illust-frames-{}-{}", session_id, stamp),
    );
    let frames_logical = normalize_download_path(&frames_logical, DownloadKind::Video);

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

    for beat in &doc.timeline.beats {
        let n = (beat.dur_s * fps_draw).round().max(1.0) as u32;
        let cam = beat.camera.clone().unwrap_or_else(|| base_cam.clone());
        let mode = beat.mode.unwrap_or(base_mode);
        for k in 0..n {
            let t = if n <= 1 {
                1.0
            } else {
                k as f32 / (n - 1) as f32
            };
            let pose = lerp_pose(&IllustrationPose::default(), &beat.pose, t);
            let png = illustration_raster::export_frame_png(&doc, &pose, &cam, mode, w, h)?;
            let name = format!("{:04}.png", frame_i);
            std::fs::write(host_frames.join(&name), &png).map_err(|e| e.to_string())?;
            frame_i += 1;
        }
    }

    let contact_logical = default_download_path(
        DownloadKind::Canvas,
        &format!("illust-contact-{}-{}.png", session_id, stamp),
    );
    let contact_logical = normalize_download_path(&contact_logical, DownloadKind::Canvas);
    let contact_bytes = build_contact_sheet(&host_frames, frame_i, 6)?;
    write_download(s, &contact_logical, &contact_bytes)?;

    let mut mp4_path = None;
    let mut message = format!("{frame_i} drawn frames @ 12fps in {frames_logical}");
    if let Some(ff) = find_ffmpeg() {
        let out_logical = path.unwrap_or_else(|| {
            default_download_path(
                DownloadKind::Video,
                &format!("illust-{}-{}.mp4", session_id, stamp),
            )
        });
        let out_logical = normalize_download_path(&out_logical, DownloadKind::Video);
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
                "12",
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
                let _ = s.sessions.lock().unwrap().illustration_set_paths(
                    session_id,
                    None,
                    None,
                    Some(out_logical),
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

fn lerp_pose(a: &IllustrationPose, b: &IllustrationPose, t: f32) -> IllustrationPose {
    let l = |x: f32, y: f32| x + (y - x) * t;
    IllustrationPose {
        walk: l(a.walk, b.walk),
        twitch: l(a.twitch, b.twitch),
        wing: l(a.wing, b.wing),
        flap: l(a.flap, b.flap),
        tuck: l(a.tuck, b.tuck),
        tilt: l(a.tilt, b.tilt),
    }
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
