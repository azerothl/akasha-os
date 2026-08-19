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
    let seed = prompt.bytes().fold(0u32, |a, b| a.wrapping_mul(16777619) ^ b as u32);
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

/// Allowlisted sd.cpp flags (P09.3). Never pass free-form argv.
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
    pub lora_path: Option<PathBuf>,
    pub clip_l_path: Option<PathBuf>,
    pub clip_g_path: Option<PathBuf>,
    pub t5xxl_path: Option<PathBuf>,
    pub diffusion_model: Option<PathBuf>,
    pub style_prefix: Option<String>,
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
            lora_path: None,
            clip_l_path: None,
            clip_g_path: None,
            t5xxl_path: None,
            diffusion_model: None,
            style_prefix: None,
        }
    }
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
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let prompt = match &opts.style_prefix {
        Some(s) if !s.is_empty() => format!("{s}, {prompt}"),
        _ => prompt.to_string(),
    };
    if stub_forced() || look_image_bin().is_none() || !weights.exists() {
        std::fs::write(dest, visible_stub_png(&prompt))?;
        return Ok(MediaEngine::Stub);
    }
    let bin = look_image_bin().expect("sd bin");
    let mut cmd = Command::new(&bin);
    cmd.current_dir(bin_dir())
        .arg("-m")
        .arg(weights)
        .arg("-p")
        .arg(&prompt)
        .arg("-o")
        .arg(dest)
        .arg("-W")
        .arg(opts.width.max(64).to_string())
        .arg("-H")
        .arg(opts.height.max(64).to_string())
        .arg("--steps")
        .arg(opts.steps.max(1).to_string());
    if let Some(cfg) = opts.cfg_scale {
        cmd.arg("--cfg-scale").arg(cfg.to_string());
    }
    if let Some(seed) = opts.seed {
        cmd.arg("--seed").arg(seed.to_string());
    }
    if let Some(method) = opts.sampling_method.as_deref().filter(|s| {
        matches!(
            *s,
            "euler" | "euler_a" | "heun" | "dpm2" | "dpm++2m" | "lcm" | "ddim"
        )
    }) {
        cmd.arg("--sampling-method").arg(method);
    }
    if let Some(neg) = opts.negative_prompt.as_deref().filter(|s| !s.is_empty()) {
        cmd.arg("-n").arg(neg);
    }
    if let Some(t) = opts.threads {
        cmd.arg("-t").arg(t.max(1).to_string());
    }
    if let Some(p) = &opts.vae_path {
        cmd.arg("--vae").arg(p);
    }
    if let Some(p) = &opts.lora_path {
        cmd.arg("--lora").arg(p);
    }
    if let Some(p) = &opts.clip_l_path {
        cmd.arg("--clip_l").arg(p);
    }
    if let Some(p) = &opts.clip_g_path {
        cmd.arg("--clip_g").arg(p);
    }
    if let Some(p) = &opts.t5xxl_path {
        cmd.arg("--t5xxl").arg(p);
    }
    if let Some(p) = &opts.diffusion_model {
        cmd.arg("--diffusion-model").arg(p);
    }
    let output = cmd.output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let tail = err.chars().rev().take(800).collect::<String>();
        let tail: String = tail.chars().rev().collect();
        return Err(MediaError::EngineFailed {
            engine: "sd".into(),
            detail: format!("exit {}: {tail}", output.status),
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
    let work = bin.parent().map(|p| p.to_path_buf()).unwrap_or_else(bin_dir);
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
        std::env::set_var("AOS_MEDIA_STUB", "1");
        let dir = std::env::temp_dir().join("aos-sd-test");
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join("out.png");
        let eng = generate_image(Path::new("missing.safetensors"), "a cat", &dest).unwrap();
        assert_eq!(eng, MediaEngine::Stub);
        let bytes = std::fs::read(&dest).unwrap();
        assert!(bytes.len() > 1024, "stub must be large enough to see in chat");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        std::env::remove_var("AOS_MEDIA_STUB");
    }

    #[test]
    fn generate_speech_stub() {
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
        std::env::set_var("AOS_MEDIA_STUB", "1");
        let dir = std::env::temp_dir().join("aos-sd-test");
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join("out-opts.png");
        let mut opts = ImageGenOpts::default();
        opts.width = 768;
        opts.steps = 8;
        let eng = generate_image_opts(Path::new("missing.safetensors"), "cube", &dest, &opts)
            .unwrap();
        assert_eq!(eng, MediaEngine::Stub);
        std::env::remove_var("AOS_MEDIA_STUB");
    }
}
