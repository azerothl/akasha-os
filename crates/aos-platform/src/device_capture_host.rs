//! Backends Linux/macOS pour la capture caméra/microphone (issue #137 slice 2).
//!
//! - Caméra : [nokhwa](https://crates.io/crates/nokhwa) (`input-native` → V4L2 ou AVFoundation)
//! - Microphone : [cpal](https://crates.io/crates/cpal) (ALSA/PulseAudio/JACK sur Linux, CoreAudio sur macOS)

use crate::device_capture::{
    encode_bgra_png, BackendCapture, BackendStream, DeviceCaptureBackend, DeviceCaptureError,
};
use aos_proto::device_capture::{DeviceDescriptor, DeviceKind, OsPermissionState};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::{query, Camera};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MIC_ONCE_MS: u64 = 250;

#[derive(Debug, Default)]
pub struct HostDeviceCaptureBackend;

impl DeviceCaptureBackend for HostDeviceCaptureBackend {
    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, DeviceCaptureError> {
        let mut devices = Vec::new();
        devices.extend(enumerate_cameras()?);
        devices.extend(enumerate_microphones()?);
        Ok(devices)
    }

    fn capture_once(
        &self,
        device: &DeviceDescriptor,
        output: &Path,
        max_bytes: u64,
    ) -> Result<BackendCapture, DeviceCaptureError> {
        let started = std::time::Instant::now();
        let (bytes, mime_type) = if device.kind == DeviceKind::Camera {
            let png = capture_camera_png(device, max_bytes)?;
            (png, "image/png".to_string())
        } else {
            let pcm = capture_mic_once(device, max_bytes)?;
            (pcm, "application/octet-stream".to_string())
        };
        std::fs::write(output, &bytes).map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
        Ok(BackendCapture {
            size_bytes: bytes.len() as u64,
            duration_ms: started.elapsed().as_millis() as u64,
            mime_type,
        })
    }

    fn start_stream(
        &self,
        device: &DeviceDescriptor,
        output: &Path,
        max_duration_ms: u64,
        max_bytes: u64,
    ) -> Result<BackendStream, DeviceCaptureError> {
        if device.kind == DeviceKind::Camera {
            start_camera_stream(device, output, max_duration_ms, max_bytes)
        } else {
            start_mic_stream(device, output, max_duration_ms, max_bytes)
        }
    }
}

fn platform_tag() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn device_index(device: &DeviceDescriptor) -> Result<usize, DeviceCaptureError> {
    let prefix = format!("{}:", platform_tag());
    if !device.id.starts_with(&prefix) {
        return Err(DeviceCaptureError::DeviceAbsent(device.id.clone()));
    }
    device
        .id
        .rsplit(':')
        .next()
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| DeviceCaptureError::DeviceAbsent(device.id.clone()))
}

fn map_host_error(error: impl std::fmt::Display) -> DeviceCaptureError {
    let msg = error.to_string().to_lowercase();
    if msg.contains("permission")
        || msg.contains("denied")
        || msg.contains("not authorized")
        || msg.contains("authorization")
        || msg.contains("access denied")
    {
        DeviceCaptureError::OsPermissionDenied
    } else if msg.contains("busy") || msg.contains("in use") || msg.contains("resource busy") {
        DeviceCaptureError::DeviceBusy
    } else if msg.contains("not found") || msg.contains("no such device") {
        DeviceCaptureError::DeviceAbsent("périphérique".into())
    } else {
        DeviceCaptureError::Backend(error.to_string())
    }
}

fn enumerate_cameras() -> Result<Vec<DeviceDescriptor>, DeviceCaptureError> {
    let infos = query(nokhwa::utils::ApiBackend::Auto).map_err(map_host_error)?;
    Ok(infos
        .into_iter()
        .enumerate()
        .map(|(index, info)| DeviceDescriptor {
            id: format!("{}:Camera:{}", platform_tag(), index),
            name: info.human_name(),
            kind: DeviceKind::Camera,
            os_permission: OsPermissionState::Unknown,
        })
        .collect())
}

fn enumerate_microphones() -> Result<Vec<DeviceDescriptor>, DeviceCaptureError> {
    let host = cpal::default_host();
    let mut devices = Vec::new();
    for (index, device) in host
        .input_devices()
        .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?
        .enumerate()
    {
        let name = device
            .name()
            .unwrap_or_else(|_| format!("Microphone {}", index + 1));
        devices.push(DeviceDescriptor {
            id: format!("{}:Microphone:{}", platform_tag(), index),
            name,
            kind: DeviceKind::Microphone,
            os_permission: OsPermissionState::Unknown,
        });
    }
    Ok(devices)
}

fn open_camera(device: &DeviceDescriptor) -> Result<Camera, DeviceCaptureError> {
    let index = device_index(device)?;
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera =
        Camera::new(CameraIndex::Index(index as u32), requested).map_err(map_host_error)?;
    camera.open_stream().map_err(map_host_error)?;
    Ok(camera)
}

fn capture_camera_png(
    device: &DeviceDescriptor,
    max_bytes: u64,
) -> Result<Vec<u8>, DeviceCaptureError> {
    let mut camera = open_camera(device)?;
    let frame = camera.frame().map_err(map_host_error)?;
    let resolution = camera.resolution();
    let width = resolution.width();
    let height = resolution.height();
    let decoded = frame.decode_image::<RgbFormat>().map_err(map_host_error)?;
    camera.stop_stream().ok();
    let rgba = image::RgbaImage::from_raw(width, height, rgb_to_rgba(decoded.into_raw()))
        .ok_or_else(|| DeviceCaptureError::Backend("frame RGB invalide".into()))?;
    let rgba = scale_for_vision(rgba, width, height);
    let mut png = Vec::new();
    rgba.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
    if (png.len() as u64) > max_bytes {
        return Err(DeviceCaptureError::QuotaExceeded("taille".into()));
    }
    Ok(png)
}

fn rgb_to_rgba(rgb: Vec<u8>) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((rgb.len() / 3) * 4);
    for chunk in rgb.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
    }
    rgba
}

fn scale_for_vision(img: image::RgbaImage, width: u32, height: u32) -> image::RgbaImage {
    const MAX_VISION_EDGE: u32 = 1280;
    if width.max(height) <= MAX_VISION_EDGE {
        return img;
    }
    let scale = MAX_VISION_EDGE as f32 / width.max(height) as f32;
    let nw = ((width as f32) * scale).round().max(1.0) as u32;
    let nh = ((height as f32) * scale).round().max(1.0) as u32;
    image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
}

fn cpal_input_device(index: usize) -> Result<cpal::Device, DeviceCaptureError> {
    let host = cpal::default_host();
    host.input_devices()
        .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?
        .nth(index)
        .ok_or_else(|| DeviceCaptureError::DeviceAbsent(format!("microphone:{index}")))
}

fn capture_mic_once(
    device: &DeviceDescriptor,
    max_bytes: u64,
) -> Result<Vec<u8>, DeviceCaptureError> {
    let index = device_index(device)?;
    let input = cpal_input_device(index)?;
    let config = input.default_input_config().map_err(map_host_error)?;
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();
    let samples: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let max = max_bytes as usize;
    let stream = build_input_stream(
        &input,
        sample_format,
        &stream_config,
        samples.clone(),
        max,
        |_| {},
    )?;
    stream.play().map_err(map_host_error)?;
    std::thread::sleep(Duration::from_millis(MIC_ONCE_MS));
    stream.pause().ok();
    drop(stream);
    let bytes = samples.lock().unwrap().clone();
    if bytes.is_empty() {
        return Err(DeviceCaptureError::Backend(
            "échantillon microphone vide".into(),
        ));
    }
    Ok(bytes)
}

fn start_camera_stream(
    device: &DeviceDescriptor,
    output: &Path,
    max_duration_ms: u64,
    max_bytes: u64,
) -> Result<BackendStream, DeviceCaptureError> {
    let stop = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let finished_thread = finished.clone();
    let device = device.clone();
    let output = output.to_path_buf();
    let join = std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let mut camera = open_camera(&device)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output)
            .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
        let mut total = 0u64;
        while !stop_thread.load(Ordering::Acquire)
            && started.elapsed().as_millis() < max_duration_ms as u128
            && total < max_bytes
        {
            let frame = camera.frame().map_err(map_host_error)?;
            let resolution = camera.resolution();
            let width = resolution.width();
            let height = resolution.height();
            let decoded = frame.decode_image::<RgbFormat>().map_err(map_host_error)?;
            let stride = (width as usize) * 4;
            let bgra = rgba_to_bgra(&rgb_to_rgba(decoded.into_raw()), width);
            let png = encode_bgra_png(width, height, stride, &bgra, false)?;
            if total + png.len() as u64 > max_bytes {
                break;
            }
            file.write_all(&png)
                .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
            total += png.len() as u64;
        }
        camera.stop_stream().ok();
        finished_thread.store(true, Ordering::Release);
        Ok(BackendCapture {
            size_bytes: total,
            duration_ms: started.elapsed().as_millis() as u64,
            mime_type: "application/octet-stream".into(),
        })
    });
    Ok(BackendStream {
        stop,
        finished,
        mime_type: "application/octet-stream".into(),
        join: Some(join),
    })
}

fn rgba_to_bgra(rgba: &[u8], _width: u32) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());
    for chunk in rgba.as_chunks::<4>().0 {
        bgra.extend_from_slice(&[chunk[2], chunk[1], chunk[0], chunk[3]]);
    }
    bgra
}

fn start_mic_stream(
    device: &DeviceDescriptor,
    output: &Path,
    max_duration_ms: u64,
    max_bytes: u64,
) -> Result<BackendStream, DeviceCaptureError> {
    let stop = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let finished_thread = finished.clone();
    let device = device.clone();
    let output = output.to_path_buf();
    let join = std::thread::spawn(move || {
        run_mic_stream(
            &device,
            &output,
            max_duration_ms,
            max_bytes,
            stop_thread,
            finished_thread,
        )
    });
    Ok(BackendStream {
        stop,
        finished,
        mime_type: "application/octet-stream".into(),
        join: Some(join),
    })
}

fn run_mic_stream(
    device: &DeviceDescriptor,
    output: &Path,
    max_duration_ms: u64,
    max_bytes: u64,
    stop_thread: Arc<AtomicBool>,
    finished_thread: Arc<AtomicBool>,
) -> Result<BackendCapture, DeviceCaptureError> {
    let started = std::time::Instant::now();
    let index = device_index(device)?;
    let input = cpal_input_device(index)?;
    let config = input.default_input_config().map_err(map_host_error)?;
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();
    let samples: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let max = max_bytes as usize;
    let stop_cb = stop_thread.clone();
    let stream = build_input_stream(
        &input,
        sample_format,
        &stream_config,
        samples.clone(),
        max,
        move |err| {
            if is_permission_error(&err.to_string()) {
                stop_cb.store(true, Ordering::Release);
            }
        },
    )?;
    stream.play().map_err(map_host_error)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output)
        .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
    while !stop_thread.load(Ordering::Acquire)
        && started.elapsed().as_millis() < max_duration_ms as u128
    {
        let chunk = drain_samples(&samples);
        if !chunk.is_empty() {
            file.write_all(&chunk)
                .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
        }
        if file.metadata().map(|m| m.len()).unwrap_or(0) >= max_bytes {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    stream.pause().ok();
    drop(stream);
    finished_thread.store(true, Ordering::Release);
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    Ok(BackendCapture {
        size_bytes: size,
        duration_ms: started.elapsed().as_millis() as u64,
        mime_type: "application/octet-stream".into(),
    })
}

fn build_input_stream(
    input: &cpal::Device,
    sample_format: SampleFormat,
    stream_config: &StreamConfig,
    samples: Arc<Mutex<Vec<u8>>>,
    max_bytes: usize,
    on_error: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, DeviceCaptureError> {
    match sample_format {
        SampleFormat::F32 => input
            .build_input_stream(
                stream_config,
                {
                    let samples = samples.clone();
                    move |data: &[f32], _| append_f32_samples(data, &samples, max_bytes)
                },
                on_error,
                None,
            )
            .map_err(map_host_error),
        SampleFormat::I16 => input
            .build_input_stream(
                stream_config,
                {
                    let samples = samples.clone();
                    move |data: &[i16], _| append_i16_samples(data, &samples, max_bytes)
                },
                on_error,
                None,
            )
            .map_err(map_host_error),
        SampleFormat::U16 => input
            .build_input_stream(
                stream_config,
                {
                    let samples = samples.clone();
                    move |data: &[u16], _| append_u16_samples(data, &samples, max_bytes)
                },
                on_error,
                None,
            )
            .map_err(map_host_error),
        other => Err(DeviceCaptureError::Backend(format!(
            "format audio non supporté: {other}"
        ))),
    }
}

fn drain_samples(samples: &Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    let mut guard = samples.lock().unwrap();
    if guard.is_empty() {
        Vec::new()
    } else {
        guard.drain(..).collect()
    }
}

fn append_f32_samples(data: &[f32], out: &Arc<Mutex<Vec<u8>>>, max_bytes: usize) {
    let mut guard = out.lock().unwrap();
    let room = max_bytes.saturating_sub(guard.len());
    if room == 0 {
        return;
    }
    let bytes = samples_to_i16_le(data);
    let take = bytes.len().min(room);
    guard.extend_from_slice(&bytes[..take]);
}

fn append_i16_samples(data: &[i16], out: &Arc<Mutex<Vec<u8>>>, max_bytes: usize) {
    let mut guard = out.lock().unwrap();
    let room = max_bytes.saturating_sub(guard.len());
    if room == 0 {
        return;
    }
    let bytes = i16_slice_to_le(data);
    let take = bytes.len().min(room);
    guard.extend_from_slice(&bytes[..take]);
}

fn append_u16_samples(data: &[u16], out: &Arc<Mutex<Vec<u8>>>, max_bytes: usize) {
    let mut guard = out.lock().unwrap();
    let room = max_bytes.saturating_sub(guard.len());
    if room == 0 {
        return;
    }
    let bytes = u16_slice_to_i16_le(data);
    let take = bytes.len().min(room);
    guard.extend_from_slice(&bytes[..take]);
}

fn is_permission_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("permission")
        || lower.contains("denied")
        || lower.contains("not authorized")
        || lower.contains("authorization")
}

fn samples_to_i16_le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let scaled = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&scaled.to_le_bytes());
    }
    out
}

fn i16_slice_to_le(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

fn u16_slice_to_i16_le(samples: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let centered = (*sample as i32 - 32768) as i16;
        out.extend_from_slice(&centered.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_device_id_parsing() {
        let device = DeviceDescriptor {
            id: format!("{}:Camera:2", platform_tag()),
            name: "Cam".into(),
            kind: DeviceKind::Camera,
            os_permission: OsPermissionState::Unknown,
        };
        assert_eq!(device_index(&device).unwrap(), 2);
        let wrong = DeviceDescriptor {
            id: "windows:Camera:0".into(),
            name: "X".into(),
            kind: DeviceKind::Camera,
            os_permission: OsPermissionState::Unknown,
        };
        assert!(matches!(
            device_index(&wrong),
            Err(DeviceCaptureError::DeviceAbsent(_))
        ));
    }

    #[test]
    fn default_backend_is_not_unsupported_on_host() {
        let backend = crate::device_capture::default_backend();
        let result = backend.enumerate();
        assert!(!matches!(
            result,
            Err(DeviceCaptureError::UnsupportedPlatform)
        ));
    }

    #[test]
    fn permission_error_mapping() {
        let err = map_host_error("AVCaptureDevice authorization denied");
        assert_eq!(err, DeviceCaptureError::OsPermissionDenied);
        let err = map_host_error("device or resource busy");
        assert_eq!(err, DeviceCaptureError::DeviceBusy);
    }
}
