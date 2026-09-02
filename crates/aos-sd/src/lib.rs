//! Local media engines for Preview 0.8 (E16).
//!
//! Image: `bin/sd` from [stable-diffusion.cpp](https://github.com/leejet/stable-diffusion.cpp).
//! TTS: `bin/piper`. When the engine binary is missing, or `AOS_MEDIA_STUB=1`,
//! a valid PNG / WAV stub is written so the cap / audit / `/downloads` path
//! can be tested without shipping CUDA sd.cpp in every artefact.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("moteur introuvable: {0}")]
    EngineMissing(String),
    #[error("échec {engine}: {detail}")]
    EngineFailed { engine: String, detail: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaEngine {
    SdCpp,
    Piper,
    Stub,
}

/// Tiny valid PNG kept for header tests. Runtime stubs use [`visible_stub_png`].
pub const STUB_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xFE, 0xD4, 0xEF, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// 640×360 PNG so the chat surface shows a real picture when sd.cpp / weights are absent.
pub fn visible_stub_png(prompt: &str) -> Vec<u8> {
    use image::{ImageBuffer, Rgb, RgbImage};
    let w = 640u32;
    let h = 360u32;
    let mut img: RgbImage = ImageBuffer::from_pixel(w, h, Rgb([18, 24, 36]));
    for x in 0..w {
        for y in 0..48 {
            img.put_pixel(x, y, Rgb([32, 96, 120]));
        }
    }
    let seed = prompt
        .bytes()
        .fold(0u32, |a, b| a.wrapping_mul(16777619) ^ b as u32);
    let bars = 8u32;
    let bw = w / bars;
    for i in 0..bars {
        let r = 80 + ((seed.wrapping_add(i.wrapping_mul(37))) % 140) as u8;
        let g = 60 + ((seed.wrapping_add(i.wrapping_mul(91))) % 160) as u8;
        let b = 90 + ((seed.wrapping_add(i.wrapping_mul(13))) % 140) as u8;
        let x0 = i * bw;
        for x in x0..(x0 + bw).min(w) {
            for y in 72..h.saturating_sub(24) {
                img.put_pixel(x, y, Rgb([r, g, b]));
            }
        }
    }
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    let _ = img.write_to(&mut cursor, image::ImageFormat::Png);
    if buf.len() < 32 {
        return STUB_PNG.to_vec();
    }
    buf
}

pub fn image_engine_available() -> bool {
    look_image_bin().is_some()
}

pub fn speech_engine_available() -> bool {
    look_bin("piper").is_some()
}

fn stub_forced() -> bool {
    matches!(
        std::env::var("AOS_MEDIA_STUB").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn bin_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn look_image_bin() -> Option<PathBuf> {
    look_bin("sd").or_else(|| look_bin("sd-cli"))
}

fn look_bin(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let candidate = bin_dir().join(&exe);
    if candidate.exists() {
        return Some(candidate);
    }
    let env_key = match name {
        "sd" | "sd-cli" => "AOS_SD_BIN",
        "piper" => "AOS_PIPER_BIN",
        _ => return which(&exe),
    };
    if let Ok(p) = std::env::var(env_key) {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    which(&exe)
}

fn which(exe: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let p = dir.join(exe);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// One LoRA applied via sd.cpp prompt tag `<lora:stem:scale>`.
#[derive(Debug, Clone)]
pub struct LoraEntry {
    pub stem: String,
    pub scale: f32,
}

/// Allowlisted sd.cpp flags (P09.3). Never pass free-form argv.
///
/// Backend assignment follows
/// [stable-diffusion.cpp backend.md](https://github.com/leejet/stable-diffusion.cpp/blob/master/docs/backend.md):
/// `--backend` (compute), `--params-backend` / `--offload-to-cpu` (weights),
/// `--auto-fit`, `--diffusion-fa`, `--max-vram`, `--stream-layers`
/// ([performance.md](https://github.com/leejet/stable-diffusion.cpp/blob/master/docs/performance.md)).
#[derive(Debug, Clone)]
pub struct ImageGenOpts {
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg_scale: Option<f32>,
    pub seed: Option<i64>,
    pub sampling_method: Option<String>,
    pub negative_prompt: Option<String>,
    pub threads: Option<u32>,
    pub vae_path: Option<PathBuf>,
    /// LoRAs applied via `<lora:stem:scale>` tags in the prompt.
    pub lora_entries: Vec<LoraEntry>,
    /// sd.cpp `--lora-model-dir` (typically `share/models/lora/`).
    pub lora_model_dir: Option<PathBuf>,
    pub clip_l_path: Option<PathBuf>,
    pub clip_g_path: Option<PathBuf>,
    pub t5xxl_path: Option<PathBuf>,
    pub diffusion_model: Option<PathBuf>,
    pub uncond_diffusion_model: Option<PathBuf>,
    pub llm_path: Option<PathBuf>,
    pub style_prefix: Option<String>,
    /// Compute backend (`cpu`, `gpu`, `cuda0`, `vulkan0`, mixed `te=cpu,diffusion=cuda0`).
    pub backend: Option<String>,
    /// Weight residency (`cpu`, `cuda0`, `disk`, mixed). Ignored when `auto_fit`.
    pub params_backend: Option<String>,
    pub offload_to_cpu: bool,
    pub diffusion_fa: bool,
    pub auto_fit: bool,
    /// sd.cpp `--max-vram`: GiB cap, negative = free VRAM minus that many GiB (`-1`),
    /// or per-device `cuda0=6,vulkan0=2`. `0` disables graph-cut segmentation.
    pub max_vram: Option<String>,
    /// sd.cpp `--stream-layers` (needs CPU params / `--offload-to-cpu`).
    pub stream_layers: bool,
    /// sd.cpp `--upscale-model` (ESRGAN `.pth` / `.safetensors`).
    pub upscale_model_path: Option<PathBuf>,
    /// sd.cpp `--upscale-repeats` (0 = disabled).
    pub upscale_repeats: u32,
    /// sd.cpp `--upscale-tile-size`.
    pub upscale_tile_size: Option<u32>,
    /// sd.cpp `-M` mode (`img_gen`, `vid_gen`, `upscale`, …).
    pub sd_mode: Option<String>,
    /// Wan / multi-stage `--high-noise-diffusion-model`.
    pub high_noise_diffusion_model: Option<PathBuf>,
    /// Flow-matching models (`--flow-shift`).
    pub flow_shift: Option<f32>,
    /// Video frame count (`--video-frames`; `1` ≈ single image for Wan/LTX).
    pub video_frames: Option<u32>,
    /// LTX `--embeddings-connectors`.
    pub embeddings_connectors: Option<PathBuf>,
    /// LTX `--audio-vae`.
    pub audio_vae_path: Option<PathBuf>,
    /// Host path for sd.cpp `--init-img` (img2img).
    pub init_image_path: Option<PathBuf>,
    /// sd.cpp `--strength` (0..=1) when `init_image_path` is set.
    pub strength: Option<f32>,
    /// Host path for sd.cpp `--mask` (inpaint; white = regenerate region).
    pub mask_image_path: Option<PathBuf>,
}

impl Default for ImageGenOpts {
    fn default() -> Self {
        Self {
            width: 512,
            height: 512,
            steps: 20,
            cfg_scale: None,
            seed: None,
            sampling_method: None,
            negative_prompt: None,
            threads: None,
            vae_path: None,
            lora_entries: Vec::new(),
            lora_model_dir: None,
            clip_l_path: None,
            clip_g_path: None,
            t5xxl_path: None,
            diffusion_model: None,
            uncond_diffusion_model: None,
            llm_path: None,
            style_prefix: None,
            backend: None,
            params_backend: None,
            offload_to_cpu: false,
            diffusion_fa: false,
            auto_fit: false,
            max_vram: None,
            stream_layers: false,
            upscale_model_path: None,
            upscale_repeats: 0,
            upscale_tile_size: None,
            sd_mode: None,
            high_noise_diffusion_model: None,
            flow_shift: None,
            video_frames: None,
            embeddings_connectors: None,
            audio_vae_path: None,
            init_image_path: None,
            strength: None,
            mask_image_path: None,
        }
    }
}

/// Default mixed compute backend for DiT + LLM packs (encoders on CPU, diffusion on GPU).
pub const DEFAULT_MIXED_BACKEND: &str = "te=cpu,llm=cpu,diffusion=gpu,vae=cpu";

/// Closed charset for `--backend` (no spaces / argv injection).
/// Maps UI aliases `mixed` / `mixte` to [`DEFAULT_MIXED_BACKEND`] (sd.cpp rejects bare `mixte`).
pub fn sanitize_backend_spec(raw: &str) -> Option<String> {
    sanitize_backend_token(raw, BackendRole::Compute)
}

/// Closed charset for `--params-backend`. Aliases `mixed` / `mixte` → `cpu`.
pub fn sanitize_params_backend_spec(raw: &str) -> Option<String> {
    sanitize_backend_token(raw, BackendRole::Params)
}

#[derive(Clone, Copy)]
enum BackendRole {
    Compute,
    Params,
}

fn sanitize_backend_token(raw: &str, role: BackendRole) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 128 {
        return None;
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '=' | ',' | '&' | '*' | '-' | '_' | '.'))
    {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if lower == "mixed" || lower == "mixte" {
        return Some(match role {
            BackendRole::Compute => DEFAULT_MIXED_BACKEND.to_string(),
            BackendRole::Params => "cpu".into(),
        });
    }
    // Reject bare unknown tokens (sd.cpp: "backend 'X' was not found").
    if !lower.contains('=') {
        let ok = matches!(
            lower.as_str(),
            "cpu" | "gpu" | "cuda" | "cuda0" | "vulkan" | "vulkan0" | "metal" | "disk"
        ) || lower.starts_with("cuda")
            || lower.starts_with("vulkan");
        if !ok {
            return None;
        }
    }
    Some(s.to_string())
}

/// Closed charset for `--max-vram` (`-1`, `8`, `cuda0=6,vulkan0=2`).
pub fn sanitize_max_vram(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 64 {
        return None;
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '=' | ',' | '-' | '_' | '.'))
    {
        return None;
    }
    Some(s.to_string())
}

#[derive(Debug, Clone, Default)]
pub struct SpeechGenOpts {
    pub length_scale: Option<f32>,
    pub noise_scale: Option<f32>,
    pub noise_w: Option<f32>,
    pub sentence_silence: Option<f32>,
    pub speaker: Option<u32>,
}

/// Generate a PNG from `prompt` using sd.cpp, or a stub PNG.
pub fn generate_image(
    weights: &Path,
    prompt: &str,
    dest: &Path,
) -> Result<MediaEngine, MediaError> {
    generate_image_opts(weights, prompt, dest, &ImageGenOpts::default())
}

pub fn generate_image_opts(
    weights: &Path,
    prompt: &str,
    dest: &Path,
    opts: &ImageGenOpts,
) -> Result<MediaEngine, MediaError> {
    generate_image_opts_progress(weights, prompt, dest, opts, |_, _| {})
}

fn prepare_lora_prompt(prompt: String, opts: &ImageGenOpts) -> (String, Option<PathBuf>) {
    if opts.lora_entries.is_empty() {
        return (prompt, opts.lora_model_dir.clone());
    }
    let mut out = prompt;
    for entry in &opts.lora_entries {
        if entry.stem.is_empty() {
            continue;
        }
        let scale = entry.scale.clamp(0.0, 2.0);
        let tag = format!("<lora:{}:{scale}>", entry.stem);
        if !out.contains(&format!("<lora:{}:", entry.stem)) {
            out = format!("{out} {tag}");
        }
    }
    (out, opts.lora_model_dir.clone())
}

fn collect_image_args(
    weights: &Path,
    prompt: &str,
    dest: &Path,
    opts: &ImageGenOpts,
    lora_model_dir: Option<&Path>,
) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(mode) = opts
        .sd_mode
        .as_deref()
        .filter(|m| !m.is_empty() && *m != "img_gen")
    {
        a.push("-M".into());
        a.push(mode.to_string());
    }
    let dit = opts.diffusion_model.is_some() || opts.uncond_diffusion_model.is_some();
    if dit {
        let dm = opts.diffusion_model.as_deref().unwrap_or(weights);
        a.push("--diffusion-model".into());
        a.push(dm.to_string_lossy().into_owned());
    } else {
        a.push("-m".into());
        a.push(weights.to_string_lossy().into_owned());
    }
    a.push("-p".into());
    a.push(prompt.to_string());
    a.push("-o".into());
    a.push(dest.to_string_lossy().into_owned());
    a.push("-W".into());
    a.push(opts.width.max(64).to_string());
    a.push("-H".into());
    a.push(opts.height.max(64).to_string());
    a.push("--steps".into());
    a.push(opts.steps.max(1).to_string());
    a.push("-v".into());
    if let Some(cfg) = opts.cfg_scale {
        a.push("--cfg-scale".into());
        a.push(cfg.to_string());
    }
    if let Some(seed) = opts.seed {
        a.push("--seed".into());
        a.push(seed.to_string());
    }
    if let Some(method) = opts.sampling_method.as_deref().filter(|s| {
        matches!(
            *s,
            "euler" | "euler_a" | "heun" | "dpm2" | "dpm++2m" | "lcm" | "ddim"
        )
    }) {
        a.push("--sampling-method".into());
        a.push(method.to_string());
    }
    if let Some(neg) = opts.negative_prompt.as_deref().filter(|s| !s.is_empty()) {
        a.push("-n".into());
        a.push(neg.to_string());
    }
    if let Some(t) = opts.threads {
        a.push("-t".into());
        a.push(t.max(1).to_string());
    }
    if let Some(p) = &opts.vae_path {
        a.push("--vae".into());
        a.push(p.to_string_lossy().into_owned());
    }
    if let Some(dir) = lora_model_dir {
        a.push("--lora-model-dir".into());
        a.push(dir.to_string_lossy().into_owned());
    }
    if let Some(p) = &opts.clip_l_path {
        a.push("--clip_l".into());
        a.push(p.to_string_lossy().into_owned());
    }
    if let Some(p) = &opts.clip_g_path {
        a.push("--clip_g".into());
        a.push(p.to_string_lossy().into_owned());
    }
    if let Some(p) = &opts.t5xxl_path {
        a.push("--t5xxl".into());
        a.push(p.to_string_lossy().into_owned());
    }
    if let Some(p) = &opts.uncond_diffusion_model {
        a.push("--uncond-diffusion-model".into());
        a.push(p.to_string_lossy().into_owned());
    }
    if let Some(p) = &opts.llm_path {
        a.push("--llm".into());
        a.push(p.to_string_lossy().into_owned());
    }
    if let Some(p) = &opts.high_noise_diffusion_model {
        a.push("--high-noise-diffusion-model".into());
        a.push(p.to_string_lossy().into_owned());
    }
    if let Some(p) = &opts.embeddings_connectors {
        a.push("--embeddings-connectors".into());
        a.push(p.to_string_lossy().into_owned());
    }
    if let Some(p) = &opts.audio_vae_path {
        a.push("--audio-vae".into());
        a.push(p.to_string_lossy().into_owned());
    }
    if let Some(fs) = opts.flow_shift {
        a.push("--flow-shift".into());
        a.push(fs.to_string());
    }
    if let Some(vf) = opts.video_frames.filter(|n| *n > 0) {
        a.push("--video-frames".into());
        a.push(vf.to_string());
    }
    if opts.auto_fit {
        a.push("--auto-fit".into());
    } else {
        if let Some(b) = opts.backend.as_deref().and_then(sanitize_backend_spec) {
            if b != "disk" {
                a.push("--backend".into());
                a.push(b);
            }
        }
        if let Some(b) = opts
            .params_backend
            .as_deref()
            .and_then(sanitize_params_backend_spec)
        {
            a.push("--params-backend".into());
            a.push(b);
        }
        if opts.offload_to_cpu {
            a.push("--offload-to-cpu".into());
        }
        if opts.stream_layers {
            a.push("--stream-layers".into());
        }
    }
    if let Some(v) = opts.max_vram.as_deref().and_then(sanitize_max_vram) {
        a.push("--max-vram".into());
        a.push(v);
    }
    if opts.diffusion_fa {
        a.push("--diffusion-fa".into());
    }
    if let Some(p) = &opts.upscale_model_path {
        if opts.upscale_repeats > 0 {
            a.push("--upscale-model".into());
            a.push(p.to_string_lossy().into_owned());
            a.push("--upscale-repeats".into());
            a.push(opts.upscale_repeats.max(1).to_string());
            if let Some(tile) = opts.upscale_tile_size {
                a.push("--upscale-tile-size".into());
                a.push(tile.clamp(32, 512).to_string());
            }
        }
    }
    if let Some(p) = &opts.init_image_path {
        if p.exists() {
            a.push("--init-img".into());
            a.push(p.to_string_lossy().into_owned());
            let strength = if opts.mask_image_path.is_some() {
                1.0
            } else {
                opts.strength.unwrap_or(0.75).clamp(0.0, 1.0)
            };
            a.push("--strength".into());
            a.push(format!("{strength:.4}"));
        }
    }
    if let Some(p) = &opts.mask_image_path {
        if p.exists() {
            if !a.iter().any(|arg| arg == "-M") {
                a.push("-M".into());
                a.push("img2img".into());
            }
            a.push("--mask".into());
            a.push(p.to_string_lossy().into_owned());
        }
    }
    a
}

fn apply_image_command(
    cmd: &mut Command,
    weights: &Path,
    prompt: &str,
    dest: &Path,
    opts: &ImageGenOpts,
    lora_model_dir: Option<&Path>,
) {
    for arg in collect_image_args(weights, prompt, dest, opts, lora_model_dir) {
        cmd.arg(arg);
    }
}

/// Like [`generate_image_opts`] but calls `on_progress(step, total_steps)` when
/// sd.cpp reports sampling progress on stdout/stderr.
pub fn generate_image_opts_progress<F>(
    weights: &Path,
    prompt: &str,
    dest: &Path,
    opts: &ImageGenOpts,
    on_progress: F,
) -> Result<MediaEngine, MediaError>
where
    F: FnMut(u32, u32) + Send + 'static,
{
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let prompt = match &opts.style_prefix {
        Some(s) if !s.is_empty() => format!("{s}, {prompt}"),
        _ => prompt.to_string(),
    };
    let (prompt, lora_model_dir) = prepare_lora_prompt(prompt, opts);
    if stub_forced() || look_image_bin().is_none() || !weights.exists() {
        std::fs::write(dest, visible_stub_png(&prompt))?;
        return Ok(MediaEngine::Stub);
    }
    let bin = look_image_bin().expect("sd bin");
    let mut cmd = Command::new(&bin);
    cmd.current_dir(bin_dir());
    apply_image_command(
        &mut cmd,
        weights,
        &prompt,
        dest,
        opts,
        lora_model_dir.as_deref(),
    );
    run_sd_cmd(cmd, dest, Some(Box::new(on_progress)))
}

fn run_sd_cmd(
    mut cmd: Command,
    dest: &Path,
    on_progress: Option<Box<dyn FnMut(u32, u32) + Send>>,
) -> Result<MediaEngine, MediaError> {
    use std::sync::mpsc;
    use std::thread;

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let (tx, rx) = mpsc::channel::<String>();
    if let Some(out) = stdout {
        let tx_out = tx.clone();
        thread::spawn(move || sd_stream_lines(out, tx_out));
    }
    if let Some(err) = stderr {
        let tx_err = tx.clone();
        thread::spawn(move || sd_stream_lines(err, tx_err));
    }
    drop(tx);

    let mut all_log = String::new();
    let mut cb = on_progress;
    while let Ok(line) = rx.recv() {
        all_log.push_str(&line);
        all_log.push('\n');
        if let Some(ref mut f) = cb {
            if let Some((step, total)) = parse_sd_step(&line) {
                f(step, total);
            }
        }
    }
    let status = child.wait()?;
    if !status.success() {
        let tail = all_log.chars().rev().take(800).collect::<String>();
        let tail: String = tail.chars().rev().collect();
        return Err(MediaError::EngineFailed {
            engine: "sd".into(),
            detail: format!("exit {}: {tail}", status),
        });
    }
    if !dest.exists() {
        return Err(MediaError::EngineFailed {
            engine: "sd".into(),
            detail: "pas de fichier de sortie".into(),
        });
    }
    Ok(MediaEngine::SdCpp)
}

/// sd.cpp progress bars often use `\r` without `\n`; split on both.
fn sd_stream_lines<R: std::io::Read + Send + 'static>(
    reader: R,
    tx: std::sync::mpsc::Sender<String>,
) {
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        let mut carry = String::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    carry.push_str(&String::from_utf8_lossy(&buf[..n]));
                    while let Some(pos) = carry.find(['\n', '\r']) {
                        let line = carry[..pos].trim().to_string();
                        carry = carry[pos + 1..].to_string();
                        while carry.starts_with('\n') || carry.starts_with('\r') {
                            carry = carry[1..].to_string();
                        }
                        if !line.is_empty() {
                            let _ = tx.send(line);
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let tail = carry.trim();
        if !tail.is_empty() {
            let _ = tx.send(tail.to_string());
        }
    });
}

#[cfg(test)]
fn parse_sd_step(line: &str) -> Option<(u32, u32)> {
    parse_sd_step_impl(line)
}

#[cfg(not(test))]
fn parse_sd_step(line: &str) -> Option<(u32, u32)> {
    parse_sd_step_impl(line)
}

/// Parse sd.cpp progress lines: legacy `step 3/20` or progress bar `| 1/3 - 2.63s/it`.
fn parse_sd_step_impl(line: &str) -> Option<(u32, u32)> {
    let line = line.trim_start_matches('\r').trim();
    if line.is_empty() {
        return None;
    }
    let lower = line.to_ascii_lowercase();
    if let Some(idx) = lower.find("step ") {
        if let Some(v) = parse_step_fraction(&line[idx + 5..]) {
            return Some(v);
        }
    }
    if let Some(idx) = line.rfind('|') {
        if let Some(v) = parse_step_fraction(line[idx + 1..].trim()) {
            return Some(v);
        }
    }
    parse_step_fraction(line)
}

fn parse_step_fraction(raw: &str) -> Option<(u32, u32)> {
    let slash = raw.find('/')?;
    let step: u32 = raw[..slash]
        .trim()
        .trim_start_matches('|')
        .trim()
        .parse()
        .ok()?;
    let after = raw[slash + 1..].trim();
    let end = after
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after.len());
    let total: u32 = after[..end].trim().parse().ok()?;
    if total == 0 || total > 500 || step > total {
        return None;
    }
    Some((step, total))
}

/// Options for sd.cpp `--mode upscale` (ESRGAN on an existing image).
#[derive(Debug, Clone)]
pub struct UpscaleOpts {
    pub upscale_model_path: PathBuf,
    pub upscale_repeats: u32,
    pub upscale_tile_size: Option<u32>,
}

fn collect_upscale_args(source: &Path, dest: &Path, opts: &UpscaleOpts) -> Vec<String> {
    let mut a = vec![
        "--mode".into(),
        "upscale".into(),
        "-i".into(),
        source.to_string_lossy().into_owned(),
        "-o".into(),
        dest.to_string_lossy().into_owned(),
        "--upscale-model".into(),
        opts.upscale_model_path.to_string_lossy().into_owned(),
        "--upscale-repeats".into(),
        opts.upscale_repeats.max(1).to_string(),
    ];
    if let Some(tile) = opts.upscale_tile_size {
        a.push("--upscale-tile-size".into());
        a.push(tile.clamp(32, 512).to_string());
    }
    a
}

/// Upscale `source` PNG/JPG to `dest` using ESRGAN via sd.cpp upscale mode.
pub fn upscale_image(
    source: &Path,
    dest: &Path,
    opts: &UpscaleOpts,
) -> Result<MediaEngine, MediaError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if stub_forced() || look_image_bin().is_none() {
        stub_upscale_image(source, dest)?;
        return Ok(MediaEngine::Stub);
    }
    if !source.is_file() {
        return Err(MediaError::EngineFailed {
            engine: "sd".into(),
            detail: format!("source image missing: {}", source.display()),
        });
    }
    if !opts.upscale_model_path.is_file() {
        return Err(MediaError::EngineFailed {
            engine: "sd".into(),
            detail: format!(
                "upscale model missing: {}",
                opts.upscale_model_path.display()
            ),
        });
    }
    let bin = look_image_bin().expect("sd bin");
    let mut cmd = Command::new(&bin);
    cmd.current_dir(bin_dir());
    for arg in collect_upscale_args(source, dest, opts) {
        cmd.arg(arg);
    }
    run_sd_cmd(cmd, dest, None)
}

fn stub_upscale_image(source: &Path, dest: &Path) -> Result<(), MediaError> {
    use image::imageops::FilterType;
    let img = image::open(source).map_err(|e| MediaError::EngineFailed {
        engine: "sd".into(),
        detail: e.to_string(),
    })?;
    let w = img.width().saturating_mul(4).max(1);
    let h = img.height().saturating_mul(4).max(1);
    let upscaled = img.resize(w, h, FilterType::Triangle);
    upscaled.save(dest).map_err(|e| MediaError::EngineFailed {
        engine: "sd".into(),
        detail: e.to_string(),
    })?;
    Ok(())
}

/// 16-bit PCM mono WAV (silence + a short click) so the clip is playable.
pub fn stub_wav(duration_ms: u32) -> Vec<u8> {
    let rate: u32 = 16_000;
    let n = (rate as u64 * duration_ms as u64 / 1000).max(160) as u32;
    let data_bytes = n * 2;
    let mut out = Vec::with_capacity(44 + data_bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36u32 + data_bytes).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for i in 0..n {
        let sample = if i < 80 { 8000i16 } else { 0 };
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Generate WAV from `text` using Piper, or a stub WAV.
pub fn generate_speech(voice: &Path, text: &str, dest: &Path) -> Result<MediaEngine, MediaError> {
    generate_speech_opts(voice, text, dest, &SpeechGenOpts::default())
}

pub fn generate_speech_opts(
    voice: &Path,
    text: &str,
    dest: &Path,
    opts: &SpeechGenOpts,
) -> Result<MediaEngine, MediaError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if stub_forced() || look_bin("piper").is_none() || !voice.exists() {
        std::fs::write(dest, stub_wav(400))?;
        return Ok(MediaEngine::Stub);
    }
    let bin = look_bin("piper").expect("piper bin");
    let work = bin
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(bin_dir);
    let mut cmd = Command::new(&bin);
    cmd.current_dir(&work)
        .arg("--model")
        .arg(voice)
        .arg("--output_file")
        .arg(dest);
    let espeak = work.join("espeak-ng-data");
    if espeak.is_dir() {
        cmd.arg("--espeak_data").arg(&espeak);
    }
    if let Some(v) = opts.length_scale {
        cmd.arg("--length_scale").arg(v.to_string());
    }
    if let Some(v) = opts.noise_scale {
        cmd.arg("--noise_scale").arg(v.to_string());
    }
    if let Some(v) = opts.noise_w {
        cmd.arg("--noise_w").arg(v.to_string());
    }
    if let Some(v) = opts.sentence_silence {
        cmd.arg("--sentence_silence").arg(v.to_string());
    }
    if let Some(v) = opts.speaker {
        cmd.arg("--speaker").arg(v.to_string());
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(MediaError::EngineFailed {
            engine: "piper".into(),
            detail: String::from_utf8_lossy(&out.stderr).into(),
        });
    }
    if !dest.exists() {
        return Err(MediaError::EngineFailed {
            engine: "piper".into(),
            detail: "pas de fichier de sortie".into(),
        });
    }
    Ok(MediaEngine::Piper)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static MEDIA_STUB_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn stub_png_est_un_png() {
        assert_eq!(&STUB_PNG[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn stub_wav_est_un_wav() {
        let w = stub_wav(200);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
    }

    #[test]
    fn generate_image_stub() {
        let _guard = MEDIA_STUB_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var("AOS_MEDIA_STUB", "1");
        let dir = std::env::temp_dir().join("aos-sd-test");
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join("out.png");
        let eng = generate_image(Path::new("missing.safetensors"), "a cat", &dest).unwrap();
        assert_eq!(eng, MediaEngine::Stub);
        let bytes = std::fs::read(&dest).unwrap();
        assert!(
            bytes.len() > 1024,
            "stub must be large enough to see in chat"
        );
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        std::env::remove_var("AOS_MEDIA_STUB");
    }

    #[test]
    fn generate_speech_stub() {
        let _guard = MEDIA_STUB_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var("AOS_MEDIA_STUB", "1");
        let dir = std::env::temp_dir().join("aos-sd-test");
        let dest = dir.join("out.wav");
        let eng = generate_speech(Path::new("missing.onnx"), "hello", &dest).unwrap();
        assert_eq!(eng, MediaEngine::Stub);
        assert!(dest.exists());
        std::env::remove_var("AOS_MEDIA_STUB");
    }

    #[test]
    fn image_opts_default_is_512_20() {
        let o = ImageGenOpts::default();
        assert_eq!(o.width, 512);
        assert_eq!(o.steps, 20);
    }

    #[test]
    fn generate_image_opts_stub_not_always_512() {
        let _guard = MEDIA_STUB_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var("AOS_MEDIA_STUB", "1");
        let dir = std::env::temp_dir().join("aos-sd-test");
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join("out-opts.png");
        let opts = ImageGenOpts {
            width: 768,
            steps: 8,
            ..Default::default()
        };
        let eng =
            generate_image_opts(Path::new("missing.safetensors"), "cube", &dest, &opts).unwrap();
        assert_eq!(eng, MediaEngine::Stub);
        std::env::remove_var("AOS_MEDIA_STUB");
    }

    #[test]
    fn image_gen_init_img_argv() {
        let dir = std::env::temp_dir().join("aos-sd-init-img");
        let _ = std::fs::create_dir_all(&dir);
        let init = dir.join("base.png");
        std::fs::write(&init, b"fake").unwrap();
        let opts = ImageGenOpts {
            init_image_path: Some(init.clone()),
            strength: Some(0.42),
            ..Default::default()
        };
        let args = collect_image_args(
            Path::new("model.safetensors"),
            "cat",
            Path::new("out.png"),
            &opts,
            None,
        );
        assert!(args.contains(&"--init-img".into()));
        assert!(args.iter().any(|a| a.ends_with("base.png")));
        assert!(args.contains(&"--strength".into()));
        assert!(args.contains(&"0.4200".into()));
    }

    #[test]
    fn image_gen_vid_gen_argv() {
        let opts = ImageGenOpts {
            sd_mode: Some("vid_gen".into()),
            video_frames: Some(33),
            flow_shift: Some(3.0),
            ..Default::default()
        };
        let args = collect_image_args(
            Path::new("wan.gguf"),
            "a cat walking",
            Path::new("out.mp4"),
            &opts,
            None,
        );
        assert!(args.contains(&"-M".into()));
        assert!(args.contains(&"vid_gen".into()));
        assert!(args.contains(&"--video-frames".into()));
        assert!(args.contains(&"33".into()));
        assert!(args.contains(&"--flow-shift".into()));
        assert!(args.contains(&"3".into()));
        assert!(args.iter().any(|a| a.ends_with("out.mp4")));
    }

    #[test]
    fn image_gen_inpaint_mask_argv() {
        let dir = std::env::temp_dir().join("aos-sd-inpaint");
        let _ = std::fs::create_dir_all(&dir);
        let init = dir.join("base.png");
        let mask = dir.join("mask.png");
        std::fs::write(&init, b"fake").unwrap();
        std::fs::write(&mask, b"fake").unwrap();
        let opts = ImageGenOpts {
            init_image_path: Some(init),
            mask_image_path: Some(mask.clone()),
            ..Default::default()
        };
        let args = collect_image_args(
            Path::new("model.safetensors"),
            "cat",
            Path::new("out.png"),
            &opts,
            None,
        );
        assert!(args.contains(&"-M".into()));
        assert!(args.contains(&"img2img".into()));
        assert!(args.contains(&"--mask".into()));
        assert!(args.iter().any(|a| a.ends_with("mask.png")));
        assert!(args.contains(&"--strength".into()));
        assert!(args.contains(&"1.0000".into()));
    }

    #[test]
    fn upscale_argv_matches_sdcpp_recipe() {
        let opts = UpscaleOpts {
            upscale_model_path: PathBuf::from("RealESRGAN_x4plus_anime_6B.pth"),
            upscale_repeats: 2,
            upscale_tile_size: Some(128),
        };
        let args = collect_upscale_args(Path::new("in.png"), Path::new("out.png"), &opts);
        assert!(args.contains(&"--mode".into()));
        assert!(args.contains(&"upscale".into()));
        assert!(args.contains(&"-i".into()));
        assert!(args.contains(&"in.png".into()));
        assert!(args.contains(&"-o".into()));
        assert!(args.contains(&"out.png".into()));
        assert!(args.contains(&"--upscale-model".into()));
        assert!(args.contains(&"--upscale-repeats".into()));
        assert!(args.contains(&"2".into()));
        assert!(args.contains(&"--upscale-tile-size".into()));
        assert!(args.contains(&"128".into()));
    }

    #[test]
    fn image_gen_upscale_argv() {
        let opts = ImageGenOpts {
            upscale_model_path: Some(PathBuf::from("RealESRGAN_x4plus_anime_6B.pth")),
            upscale_repeats: 1,
            upscale_tile_size: Some(128),
            ..Default::default()
        };
        let args = collect_image_args(
            Path::new("model.safetensors"),
            "a cat",
            Path::new("out.png"),
            &opts,
            None,
        );
        assert!(args.contains(&"--upscale-model".into()));
        assert!(args
            .iter()
            .any(|x| x.contains("RealESRGAN_x4plus_anime_6B.pth")));
        assert!(args.contains(&"--upscale-repeats".into()));
        assert!(args.contains(&"1".into()));
        assert!(args.contains(&"--upscale-tile-size".into()));
        assert!(args.contains(&"128".into()));
    }

    #[test]
    fn ideogram_argv_matches_sdcpp_recipe() {
        let opts = ImageGenOpts {
            width: 1024,
            height: 1024,
            steps: 28,
            diffusion_model: Some(PathBuf::from("ideogram4-Q4_0.gguf")),
            uncond_diffusion_model: Some(PathBuf::from("ideogram4_uncond-Q4_0.gguf")),
            llm_path: Some(PathBuf::from("Qwen3-VL-8B-Instruct-Q4_K_M.gguf")),
            vae_path: Some(PathBuf::from("flux2_ae.safetensors")),
            offload_to_cpu: true,
            diffusion_fa: true,
            max_vram: Some("-1".into()),
            stream_layers: true,
            ..Default::default()
        };
        let args = collect_image_args(
            Path::new("ideogram4-Q4_0.gguf"),
            r#"{"high_level_description":"a cat"}"#,
            Path::new("out.png"),
            &opts,
            None,
        );
        assert!(args.contains(&"--diffusion-model".into()));
        assert!(args.contains(&"--uncond-diffusion-model".into()));
        assert!(args.contains(&"--llm".into()));
        assert!(args.contains(&"--vae".into()));
        assert!(args.contains(&"--diffusion-fa".into()));
        assert!(args.contains(&"--offload-to-cpu".into()));
        assert!(args.contains(&"--max-vram".into()));
        assert!(args.contains(&"-1".into()));
        assert!(args.contains(&"--stream-layers".into()));
        assert!(!args.iter().any(|x| x == "-m"));
        assert_eq!(sanitize_max_vram("-1").as_deref(), Some("-1"));
        assert_eq!(sanitize_max_vram("cuda0=8").as_deref(), Some("cuda0=8"));
        assert!(sanitize_max_vram("8;rm").is_none());
        assert_eq!(
            sanitize_backend_spec("te=cpu,diffusion=cuda0").as_deref(),
            Some("te=cpu,diffusion=cuda0")
        );
        assert!(sanitize_backend_spec("cpu; rm -rf").is_none());
        assert_eq!(
            sanitize_backend_spec("mixte").as_deref(),
            Some(DEFAULT_MIXED_BACKEND)
        );
        assert_eq!(
            sanitize_backend_spec("mixed").as_deref(),
            Some(DEFAULT_MIXED_BACKEND)
        );
        assert_eq!(
            sanitize_params_backend_spec("mixte").as_deref(),
            Some("cpu")
        );
        assert!(sanitize_backend_spec("bogus").is_none());
        assert!(sanitize_params_backend_spec("bogus").is_none());
    }

    #[test]
    fn lora_uses_prompt_tag_and_model_dir() {
        let mut opts = ImageGenOpts::default();
        opts.lora_entries.push(LoraEntry {
            stem: "marblesh".into(),
            scale: 0.8,
        });
        opts.lora_model_dir = Some(PathBuf::from("share/models/lora"));
        let (prompt, dir) = prepare_lora_prompt("a lovely cat".into(), &opts);
        assert!(prompt.contains("<lora:marblesh:0.8>"));
        assert_eq!(dir.as_deref(), Some(Path::new("share/models/lora")));
        let args = collect_image_args(
            Path::new("sd-v1-5.safetensors"),
            &prompt,
            Path::new("out.png"),
            &opts,
            dir.as_deref(),
        );
        assert!(args.contains(&"--lora-model-dir".into()));
        assert!(args.iter().any(|x| x.contains("share/models/lora")));
        assert!(!args.iter().any(|x| x == "--lora"));
    }

    #[test]
    fn parse_sd_step_progress_bar() {
        assert_eq!(parse_sd_step_impl("| 1/3 - 2.63s/it"), Some((1, 3)));
        assert_eq!(parse_sd_step_impl("| 2/28 - 1.50s/it"), Some((2, 28)));
        assert_eq!(parse_sd_step_impl("step 5/20 sampling"), Some((5, 20)));
        assert_eq!(parse_sd_step_impl("\r| 12/28 - 0.9s/it"), Some((12, 28)));
        assert!(parse_sd_step_impl("loading weights").is_none());
    }

    #[test]
    fn multiple_loras_append_tags() {
        let mut opts = ImageGenOpts::default();
        opts.lora_entries.push(LoraEntry {
            stem: "style_a".into(),
            scale: 1.0,
        });
        opts.lora_entries.push(LoraEntry {
            stem: "style_b".into(),
            scale: 0.5,
        });
        opts.lora_model_dir = Some(PathBuf::from("share/models/lora"));
        let (prompt, _) = prepare_lora_prompt("portrait".into(), &opts);
        assert!(prompt.contains("<lora:style_a:1>"));
        assert!(prompt.contains("<lora:style_b:0.5>"));
    }
}
