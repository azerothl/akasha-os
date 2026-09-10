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
            fast: ImageModelPreset {
                width: 768,
                height: 432,
                steps: 6,
                cfg: 6.0,
                sampler: "euler",
            },
            balanced: ImageModelPreset {
                width: 1280,
                height: 720,
                steps: 8,
                cfg: 6.0,
                sampler: "euler",
            },
            quality: ImageModelPreset {
                width: 1280,
                height: 720,
                steps: 12,
                cfg: 6.5,
                sampler: "heun",
            },
        },
        "local:minimax-h3" => PresetTriplet {
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
                width: 864,
                height: 480,
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
