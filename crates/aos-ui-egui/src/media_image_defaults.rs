//! Default `media.image.generate` options for chat slash and agents (no Create UI).

use aos_proto::MediaImageOptions;

#[derive(Debug, Clone)]
pub struct ImageGenUiState {
    pub enriching: bool,
    pub upscaling: bool,
    pub step: u32,
    pub total_steps: u32,
    pub elapsed_secs: u64,
}

#[derive(Debug, Clone, Copy)]
struct ImageModelPreset {
    width: u32,
    height: u32,
    steps: u32,
    cfg: f32,
    sampler: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct PresetTriplet {
    fast: ImageModelPreset,
    balanced: ImageModelPreset,
    quality: ImageModelPreset,
}

fn image_model_presets(model_id: &str) -> PresetTriplet {
    match model_id {
        "local:flux2" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 3,
                cfg: 1.0,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 4,
                cfg: 1.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 6,
                cfg: 1.2,
                sampler: "heun",
            },
        },
        "local:sd-v1-5" => PresetTriplet {
            fast: ImageModelPreset {
                width: 512,
                height: 512,
                steps: 16,
                cfg: 6.5,
                sampler: "euler_a",
            },
            balanced: ImageModelPreset {
                width: 512,
                height: 512,
                steps: 24,
                cfg: 7.0,
                sampler: "euler_a",
            },
            quality: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 32,
                cfg: 7.5,
                sampler: "heun",
            },
        },
        "local:ideogram4" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 18,
                cfg: 4.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 28,
                cfg: 5.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 36,
                cfg: 5.5,
                sampler: "heun",
            },
        },
        "local:sdxl-base" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 20,
                cfg: 7.0,
                sampler: "euler_a",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 25,
                cfg: 7.0,
                sampler: "euler_a",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 32,
                cfg: 7.5,
                sampler: "heun",
            },
        },
        "local:z-image-turbo" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 6,
                cfg: 1.0,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 8,
                cfg: 1.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 12,
                cfg: 1.2,
                sampler: "heun",
            },
        },
        "local:qwen-image-2512" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 16,
                cfg: 2.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 20,
                cfg: 2.5,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 28,
                cfg: 3.0,
                sampler: "heun",
            },
        },
        "local:krea2-raw" => PresetTriplet {
            fast: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 12,
                cfg: 3.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 16,
                cfg: 4.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1024,
                height: 1024,
                steps: 24,
                cfg: 4.5,
                sampler: "heun",
            },
        },
        "local:wan2.2-t2i" => PresetTriplet {
            fast: ImageModelPreset {
                width: 832,
                height: 480,
                steps: 8,
                cfg: 3.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 832,
                height: 480,
                steps: 10,
                cfg: 3.5,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1280,
                height: 720,
                steps: 14,
                cfg: 4.0,
                sampler: "heun",
            },
        },
        "local:ltx2.3-dev" => PresetTriplet {
            // Align with catalogue video_defaults / AK-024 (768×512 base).
            fast: ImageModelPreset {
                width: 768,
                height: 512,
                steps: 6,
                cfg: 6.0,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 768,
                height: 512,
                steps: 8,
                cfg: 6.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 768,
                height: 512,
                steps: 12,
                cfg: 6.5,
                sampler: "heun",
            },
        },
        "local:minimax-h3" => PresetTriplet {
            // Distilled: cfg must stay 1.0 (catalogue engine_args).
            fast: ImageModelPreset {
                width: 640,
                height: 384,
                steps: 4,
                cfg: 1.0,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 864,
                height: 480,
                steps: 4,
                cfg: 1.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1344,
                height: 768,
                steps: 8,
                cfg: 1.0,
                sampler: "euler",
            },
        },
        _ => PresetTriplet {
            fast: ImageModelPreset {
                width: 512,
                height: 512,
                steps: 12,
                cfg: 6.5,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 512,
                height: 512,
                steps: 20,
                cfg: 7.0,
                sampler: "",
            },
            quality: ImageModelPreset {
                width: 768,
                height: 768,
                steps: 28,
                cfg: 7.5,
                sampler: "heun",
            },
        },
    }
}

fn pick_preset(model_id: &str, profile: &str) -> ImageModelPreset {
    let triplet = image_model_presets(model_id);
    match profile {
        "fast" => triplet.fast,
        "quality" => triplet.quality,
        _ => triplet.balanced,
    }
}

pub fn default_video_download_path() -> String {
    format!(
        "/downloads/video-{}.webm",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    )
}

pub fn is_video_options(opts: &MediaImageOptions) -> bool {
    opts.sd_mode.as_deref() == Some("vid_gen") || opts.video_frames.unwrap_or(0) > 1
}

/// Frame counts for short clips. Wan/LTX use ~16 fps with 4n+1; MiniMax-H3
/// runs at 24 fps on the 17k+5 grid (sd.cpp also aligns upward).
pub fn video_frames_for_duration_model(seconds: u32, model_id: &str) -> u32 {
    if model_id.contains("minimax") {
        return match seconds {
            2 => 56,
            4 => 90,
            _ => 73,
        };
    }
    match seconds {
        2 => 33,
        4 => 65,
        _ => 49,
    }
}

pub fn image_options_for_model(model_id: Option<&str>, profile: Option<&str>) -> MediaImageOptions {
    let id = model_id.unwrap_or_default();
    let preset = pick_preset(id, profile.unwrap_or("balanced"));
    let wants_perf = crate::image_prompt::is_heavy_image_model(id);
    MediaImageOptions {
        width: Some(preset.width),
        height: Some(preset.height),
        steps: Some(preset.steps),
        cfg_scale: Some(preset.cfg),
        sampling_method: if preset.sampler.is_empty() {
            None
        } else {
            Some(preset.sampler.to_string())
        },
        offload_to_cpu: if wants_perf { Some(true) } else { None },
        diffusion_fa: if wants_perf { Some(true) } else { None },
        max_vram: if wants_perf { Some("-1".into()) } else { None },
        stream_layers: if wants_perf { Some(true) } else { None },
        ..MediaImageOptions::default()
    }
}

/// Full Create DeclUI defaults for a model + quality profile + image/video mode.
/// Merges hardcoded recipes with catalogue `video_defaults` / `engine_args` so
/// picking a pack is enough for a sane one-click generate (prompt only).
#[derive(Debug, Clone)]
pub struct CreateGenerationDefaults {
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg_scale: f32,
    pub sampling_method: String,
    pub flow_shift: Option<f32>,
    pub video_frames: Option<u32>,
    pub fps: Option<u32>,
    /// When `Some`, DeclUI `format` is forced (video → `custom` to keep pack aspect).
    pub format: Option<&'static str>,
    pub offload_to_cpu: Option<bool>,
    pub diffusion_fa: Option<bool>,
    pub max_vram: Option<String>,
    pub stream_layers: Option<bool>,
    /// sd.cpp `-M` mode from catalogue (`img_gen` / `vid_gen` / …).
    pub sd_mode: String,
}

fn parse_engine_f32(args: &std::collections::HashMap<String, String>, key: &str) -> Option<f32> {
    args.get(key)?.trim().parse().ok()
}

fn parse_engine_bool(args: &std::collections::HashMap<String, String>, key: &str) -> Option<bool> {
    match args.get(key)?.trim() {
        "1" | "true" | "True" | "yes" => Some(true),
        "0" | "false" | "False" | "no" => Some(false),
        _ => None,
    }
}

fn clamp_u32(value: u32, min: Option<u32>, max: Option<u32>) -> u32 {
    let mut v = value;
    if let Some(lo) = min {
        v = v.max(lo);
    }
    if let Some(hi) = max {
        v = v.min(hi);
    }
    v
}

fn video_frames_for_profile(model_id: &str, profile: &str, catalog_frames: Option<u32>) -> u32 {
    let seconds = match profile {
        "fast" => 2,
        "quality" => 4,
        _ => 3,
    };
    let from_duration = video_frames_for_duration_model(seconds, model_id);
    match profile {
        // Prefer the catalogue ceiling when the user asks for quality.
        "quality" => catalog_frames.unwrap_or(from_duration).max(from_duration),
        "fast" => from_duration,
        // Balanced: short practical clip, never longer than the pack recipe.
        _ => catalog_frames
            .map(|frames| frames.min(from_duration))
            .unwrap_or(from_duration),
    }
}

fn catalogue_sd_mode(
    media_mode: &str,
    engine: Option<&std::collections::HashMap<String, String>>,
) -> String {
    if media_mode == "video" {
        return "vid_gen".into();
    }
    if let Some(mode) = engine
        .and_then(|args| args.get("mode"))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return mode.to_string();
    }
    "img_gen".into()
}

pub fn create_generation_defaults(
    model_id: Option<&str>,
    profile: Option<&str>,
    media_mode: &str,
) -> CreateGenerationDefaults {
    let id = model_id.unwrap_or_default();
    let profile = profile.unwrap_or("balanced");
    let base = image_options_for_model(Some(id), Some(profile));
    let catalog = crate::models_page::catalog_model_by_id(id);
    let video = catalog.as_ref().and_then(|m| m.video_defaults.as_ref());
    let engine = catalog.as_ref().map(|m| &m.engine_args);
    let sd_mode = catalogue_sd_mode(media_mode, engine);

    let mut width = base.width.unwrap_or(512);
    let mut height = base.height.unwrap_or(512);
    let mut cfg = base.cfg_scale.unwrap_or(7.0);
    let sampler = base.sampling_method.clone().unwrap_or_default();
    let mut flow_shift = None;
    let mut video_frames = None;
    let mut fps = None;
    let mut format = None;
    let mut offload = base.offload_to_cpu;
    let mut diffusion_fa = base.diffusion_fa;
    let mut max_vram = base.max_vram.clone();
    let mut stream_layers = base.stream_layers;

    if media_mode == "video" {
        // Keep pack aspect ratio — Create's default format is 1:1 which would
        // squash Wan/LTX/MiniMax into a square and wreck quality.
        format = Some("custom");
        if let Some(v) = video {
            if let Some(w) = v.width {
                width = w;
            }
            if let Some(h) = v.height {
                height = h;
            }
            fps = v.fps.or(Some(24));
            video_frames = Some(video_frames_for_profile(id, profile, v.frames));
        } else {
            fps = Some(if id.contains("wan") { 16 } else { 24 });
            video_frames = Some(video_frames_for_profile(id, profile, None));
        }
        if let Some(args) = engine {
            if let Some(v) = parse_engine_f32(args, "flow-shift") {
                flow_shift = Some(v);
            }
            if let Some(v) = parse_engine_f32(args, "cfg-scale") {
                cfg = v;
            }
            if let Some(v) = args.get("fps").and_then(|s| s.trim().parse().ok()) {
                fps = Some(v);
            }
            offload = parse_engine_bool(args, "offload-to-cpu").or(offload);
            diffusion_fa = parse_engine_bool(args, "diffusion-fa").or(diffusion_fa);
            stream_layers = parse_engine_bool(args, "stream-layers").or(stream_layers);
            if let Some(v) = args.get("max-vram").filter(|s| !s.trim().is_empty()) {
                max_vram = Some(v.clone());
            }
        }
        if flow_shift.is_none()
            && (id.contains("wan")
                || id.contains("ltx")
                || id.contains("flux")
                || id.contains("qwen-image"))
        {
            flow_shift = Some(3.0);
        }
        // Video packs are heavy; prefer the catalogue offload recipe when unset.
        if offload.is_none() {
            offload = Some(true);
            diffusion_fa = Some(true);
            stream_layers = Some(true);
            max_vram = Some("-1".into());
        }
    } else if let Some(args) = engine {
        if let Some(v) = parse_engine_f32(args, "flow-shift") {
            flow_shift = Some(v);
        }
        if let Some(v) = parse_engine_f32(args, "cfg-scale") {
            cfg = v;
        }
        offload = parse_engine_bool(args, "offload-to-cpu").or(offload);
        diffusion_fa = parse_engine_bool(args, "diffusion-fa").or(diffusion_fa);
        stream_layers = parse_engine_bool(args, "stream-layers").or(stream_layers);
        if let Some(v) = args.get("max-vram").filter(|s| !s.trim().is_empty()) {
            max_vram = Some(v.clone());
        }
        // Wan-as-image still ships video_defaults for a sane canvas size.
        if let Some(v) = video {
            if let Some(w) = v.width {
                width = w;
            }
            if let Some(h) = v.height {
                height = h;
            }
        }
    }

    // Honour catalogue resolution / duration caps whenever present.
    if let Some(v) = video {
        width = clamp_u32(width, v.min_width, v.max_width);
        height = clamp_u32(height, v.min_height, v.max_height);
        if let Some(max_secs) = v.max_duration_secs {
            let fps_u = fps.unwrap_or(if id.contains("wan") { 16 } else { 24 }).max(1);
            let mut cap = max_secs.saturating_mul(fps_u);
            // Wan / LTX expect 4n+1 frame counts.
            if id.contains("wan") || id.contains("ltx") {
                if cap > 1 {
                    cap = ((cap.saturating_sub(1)) / 4) * 4 + 1;
                }
            }
            if let Some(frames) = video_frames.as_mut() {
                *frames = (*frames).min(cap.max(1));
            }
        }
    }

    CreateGenerationDefaults {
        width,
        height,
        steps: base.steps.unwrap_or(20),
        cfg_scale: cfg,
        sampling_method: sampler,
        flow_shift,
        video_frames,
        fps,
        format,
        offload_to_cpu: offload,
        diffusion_fa,
        max_vram,
        stream_layers,
        sd_mode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_workspace_home<T>(f: impl FnOnce() -> T) -> T {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let previous = std::env::var_os("AOS_HOME");
        std::env::set_var("AOS_HOME", &root);
        let out = f();
        match previous {
            Some(value) => std::env::set_var("AOS_HOME", value),
            None => std::env::remove_var("AOS_HOME"),
        }
        out
    }

    #[test]
    fn ltx_balanced_matches_catalogue_aspect_not_square() {
        with_workspace_home(|| {
            let d =
                create_generation_defaults(Some("local:ltx2.3-dev"), Some("balanced"), "video");
            assert_eq!((d.width, d.height), (768, 512));
            assert_eq!(d.format, Some("custom"));
            assert_eq!(d.fps, Some(24));
            assert!(d.video_frames.unwrap_or(0) >= 33);
            assert_eq!(d.cfg_scale, 6.0);
        });
    }

    #[test]
    fn wan_video_defaults_use_16fps_grid() {
        with_workspace_home(|| {
            let d =
                create_generation_defaults(Some("local:wan2.2-t2i"), Some("balanced"), "video");
            assert_eq!((d.width, d.height), (832, 480));
            assert_eq!(d.fps, Some(16));
            assert_eq!(d.flow_shift, Some(3.0));
            let frames = d.video_frames.unwrap_or(0);
            assert_eq!(frames % 4, 1, "Wan frames must be 4n+1, got {frames}");
            assert_eq!(d.sd_mode, "vid_gen");
        });
    }

    #[test]
    fn minimax_keeps_cfg_one() {
        with_workspace_home(|| {
            let d =
                create_generation_defaults(Some("local:minimax-h3"), Some("balanced"), "video");
            assert_eq!(d.cfg_scale, 1.0);
            assert_eq!(d.format, Some("custom"));
            assert_eq!(d.sd_mode, "vid_gen");
        });
    }

    #[test]
    fn image_mode_does_not_force_custom_format() {
        let d = create_generation_defaults(Some("local:sd-v1-5"), Some("balanced"), "image");
        assert_eq!((d.width, d.height), (512, 512));
        assert!(d.format.is_none());
        assert!(d.video_frames.is_none());
        assert_eq!(d.steps, 24);
        assert_eq!(d.cfg_scale, 7.0);
        assert_eq!(d.sd_mode, "img_gen");
    }

    #[test]
    fn wan_image_mode_honors_catalogue_vid_gen() {
        with_workspace_home(|| {
            let d =
                create_generation_defaults(Some("local:wan2.2-t2i"), Some("balanced"), "image");
            assert_eq!(d.sd_mode, "vid_gen");
            assert_eq!((d.width, d.height), (832, 480));
        });
    }

    #[test]
    fn video_defaults_clamp_to_catalogue_bounds() {
        with_workspace_home(|| {
            let d =
                create_generation_defaults(Some("local:wan2.2-t2i"), Some("quality"), "video");
            assert!(d.width <= 1280);
            assert!(d.height <= 1280);
            // max_duration_secs=5 @ 16fps → at most 81 frames (4n+1)
            assert!(d.video_frames.unwrap_or(0) <= 81);
        });
    }
}
