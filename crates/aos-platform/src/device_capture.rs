//! Capture de caméra/microphone, registre d'artefacts et abstraction de
//! backend (issue #137).
//!
//! Le manager ne laisse jamais le backend choisir le chemin de sortie. Les
//! backends reçoivent un chemin déjà confiné sous la session et ne peuvent
//! donc pas écrire dans `/documents`, le profil utilisateur ou le réseau.

use aos_proto::device_capture::{
    capability_for, CaptureMetadata, CaptureMode, CapturePermission, CaptureState, DeviceArtifact,
    DeviceCaptureRequest, DeviceCaptureResponse, DeviceDescriptor, DeviceKind, OsPermissionState,
};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const MAX_CAPTURE_DURATION_MS: u64 = 60_000;
pub const MAX_CAPTURE_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_CAPTURES_PER_SESSION: usize = 32;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DeviceCaptureError {
    #[error("plateforme non supportée pour la capture caméra/microphone")]
    UnsupportedPlatform,
    #[error("périphérique absent: {0}")]
    DeviceAbsent(String),
    #[error("permission Windows refusée pour le périphérique")]
    OsPermissionDenied,
    #[error("périphérique occupé")]
    DeviceBusy,
    #[error("quota de capture atteint: {0}")]
    QuotaExceeded(String),
    #[error("capture inconnue: {0}")]
    CaptureNotFound(String),
    #[error("requête de capture invalide: {0}")]
    InvalidRequest(String),
    #[error("backend: {0}")]
    Backend(String),
    #[error("artefact: {0}")]
    Artifact(String),
}

#[derive(Debug, Clone)]
pub struct BackendCapture {
    pub size_bytes: u64,
    pub duration_ms: u64,
    pub mime_type: String,
}

/// Contrôle minimal d'un flux ouvert par le backend.
#[derive(Debug)]
pub struct BackendStream {
    pub stop: Arc<AtomicBool>,
    pub finished: Arc<AtomicBool>,
    pub mime_type: String,
    pub join: Option<std::thread::JoinHandle<Result<BackendCapture, DeviceCaptureError>>>,
}

pub trait DeviceCaptureBackend: Send + Sync {
    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, DeviceCaptureError>;
    fn capture_once(
        &self,
        device: &DeviceDescriptor,
        output: &Path,
        max_bytes: u64,
    ) -> Result<BackendCapture, DeviceCaptureError>;
    fn start_stream(
        &self,
        device: &DeviceDescriptor,
        output: &Path,
        max_duration_ms: u64,
        max_bytes: u64,
    ) -> Result<BackendStream, DeviceCaptureError>;
}

/// Backend explicitement sûr sur Linux/macOS en slice 1.
#[derive(Debug, Default)]
pub struct UnsupportedPlatformBackend;

impl DeviceCaptureBackend for UnsupportedPlatformBackend {
    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, DeviceCaptureError> {
        Err(DeviceCaptureError::UnsupportedPlatform)
    }

    fn capture_once(
        &self,
        _device: &DeviceDescriptor,
        _output: &Path,
        _max_bytes: u64,
    ) -> Result<BackendCapture, DeviceCaptureError> {
        Err(DeviceCaptureError::UnsupportedPlatform)
    }

    fn start_stream(
        &self,
        _device: &DeviceDescriptor,
        _output: &Path,
        _max_duration_ms: u64,
        _max_bytes: u64,
    ) -> Result<BackendStream, DeviceCaptureError> {
        Err(DeviceCaptureError::UnsupportedPlatform)
    }
}

/// Point d'intégration Windows Media Foundation/COM.
///
/// La séparation est volontaire : le contrat et tous les tests restent
/// portables, tandis que l'implémentation native peut évoluer sans toucher
/// aux intents. Le backend refuse explicitement l'ouverture tant que le
/// composant natif n'est pas disponible, ce qui garantit un échec fermé.
#[cfg(windows)]
#[derive(Debug, Default)]
pub struct WindowsMediaFoundationBackend;

#[cfg(windows)]
impl DeviceCaptureBackend for WindowsMediaFoundationBackend {
    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, DeviceCaptureError> {
        enumerate_windows_devices()
    }

    fn capture_once(
        &self,
        device: &DeviceDescriptor,
        output: &Path,
        max_bytes: u64,
    ) -> Result<BackendCapture, DeviceCaptureError> {
        let started = std::time::Instant::now();
        let (sample, mime_type) = if device.kind == DeviceKind::Camera {
            let png = windows_camera_png(device, max_bytes)?;
            (png, "image/png".to_string())
        } else {
            (
                windows_sample(device, max_bytes)?,
                "application/octet-stream".to_string(),
            )
        };
        fs::write(output, &sample).map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
        Ok(BackendCapture {
            size_bytes: sample.len() as u64,
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
        let stop = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let finished_thread = finished.clone();
        let device = device.clone();
        let output = output.to_path_buf();
        let join = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let mut total = 0u64;
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(output)
                .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
            while !stop_thread.load(Ordering::Acquire)
                && started.elapsed().as_millis() < max_duration_ms as u128
                && total < max_bytes
            {
                let sample = windows_sample(&device, max_bytes - total)?;
                if sample.is_empty() {
                    break;
                }
                file.write_all(&sample)
                    .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
                total += sample.len() as u64;
            }
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
}

#[cfg(windows)]
fn mf_error(error: impl std::fmt::Display) -> DeviceCaptureError {
    DeviceCaptureError::Backend(format!("Media Foundation/COM: {error}"))
}

#[cfg(windows)]
fn enumerate_windows_devices() -> Result<Vec<DeviceDescriptor>, DeviceCaptureError> {
    use std::ptr::null_mut;
    use windows::core::PWSTR;
    use windows::Win32::Media::MediaFoundation::{
        MFCreateAttributes, MFEnumDeviceSources, MFShutdown, MFStartup, MFSTARTUP_FULL,
        MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_AUDCAP_GUID,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_VERSION,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
    };

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(mf_error)?;
        MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(mf_error)?;
        let result = (|| {
            let mut devices = Vec::new();
            for (kind, source_type) in [
                (
                    DeviceKind::Camera,
                    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                ),
                (
                    DeviceKind::Microphone,
                    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_AUDCAP_GUID,
                ),
            ] {
                let mut attrs = None;
                MFCreateAttributes(&mut attrs, 1).map_err(mf_error)?;
                let attrs = attrs.ok_or_else(|| mf_error("attributs absents"))?;
                attrs
                    .SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &source_type)
                    .map_err(mf_error)?;
                let mut raw: *mut Option<windows::Win32::Media::MediaFoundation::IMFActivate> =
                    null_mut();
                let mut count = 0u32;
                MFEnumDeviceSources(&attrs, &mut raw, &mut count).map_err(mf_error)?;
                if !raw.is_null() {
                    let entries = std::slice::from_raw_parts_mut(raw, count as usize);
                    for (index, entry) in entries.iter_mut().enumerate() {
                        if let Some(activate) = entry.take() {
                            let mut name = PWSTR::null();
                            let mut len = 0u32;
                            let name = if activate
                                .GetAllocatedString(
                                    &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                                    &mut name,
                                    &mut len,
                                )
                                .is_ok()
                            {
                                let value = name
                                    .to_string()
                                    .unwrap_or_else(|_| "Périphérique Windows".into());
                                CoTaskMemFree(Some(name.as_ptr() as *const _));
                                value
                            } else {
                                "Périphérique Windows".into()
                            };
                            // L'index est stable uniquement pour cette
                            // énumération ; l'identifiant public reste opaque.
                            devices.push(DeviceDescriptor {
                                id: format!("windows:{kind:?}:{index}"),
                                name,
                                kind,
                                os_permission: OsPermissionState::Unknown,
                            });
                        }
                    }
                    CoTaskMemFree(Some(raw as *const _));
                }
            }
            Ok(devices)
        })();
        MFShutdown().ok();
        CoUninitialize();
        result
    }
}

#[cfg(windows)]
fn windows_sample(
    device: &DeviceDescriptor,
    max_bytes: u64,
) -> Result<Vec<u8>, DeviceCaptureError> {
    use std::ptr::null_mut;
    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, IMFMediaSource, MFCreateAttributes, MFCreateSourceReaderFromMediaSource,
        MFEnumDeviceSources, MFShutdown, MFStartup, MFSTARTUP_FULL,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_AUDCAP_GUID,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
        MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
    };
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(mf_error)?;
        MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(mf_error)?;
        let result = (|| {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 1).map_err(mf_error)?;
            let attrs = attrs.ok_or_else(|| mf_error("attributs absents"))?;
            let source_type = if device.kind == DeviceKind::Camera {
                MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID
            } else {
                MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_AUDCAP_GUID
            };
            attrs
                .SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &source_type)
                .map_err(mf_error)?;
            let mut raw: *mut Option<IMFActivate> = null_mut();
            let mut count = 0u32;
            MFEnumDeviceSources(&attrs, &mut raw, &mut count).map_err(mf_error)?;
            let wanted_index = device
                .id
                .rsplit(':')
                .next()
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0);
            let mut selected = None;
            if !raw.is_null() {
                let entries = std::slice::from_raw_parts_mut(raw, count as usize);
                for (index, entry) in entries.iter_mut().enumerate() {
                    if let Some(activate) = entry.take() {
                        if index == wanted_index {
                            selected = Some(activate);
                            break;
                        }
                    }
                }
                CoTaskMemFree(Some(raw as *const _));
            }
            let activate =
                selected.ok_or_else(|| DeviceCaptureError::DeviceAbsent(device.id.clone()))?;
            let source: IMFMediaSource = activate.ActivateObject().map_err(mf_error)?;
            let reader = MFCreateSourceReaderFromMediaSource(&source, None).map_err(mf_error)?;
            let stream = if device.kind == DeviceKind::Camera {
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32
            } else {
                MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32
            };
            reader.SetStreamSelection(stream, true).map_err(mf_error)?;
            let mut flags = 0u32;
            let mut sample = None;
            reader
                .ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))
                .map_err(mf_error)?;
            let sample = sample.ok_or_else(|| {
                DeviceCaptureError::Backend("sample Media Foundation vide".into())
            })?;
            let buffer = sample.ConvertToContiguousBuffer().map_err(mf_error)?;
            let mut ptr = null_mut();
            let mut current = 0u32;
            buffer
                .Lock(&mut ptr, None, Some(&mut current))
                .map_err(mf_error)?;
            let size = (current as u64).min(max_bytes) as usize;
            let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
            buffer.Unlock().map_err(mf_error)?;
            source.Shutdown().ok();
            Ok(bytes)
        })();
        MFShutdown().ok();
        CoUninitialize();
        result
    }
}

/// Convertit un tampon BGRA/BGRX (Media Foundation RGB32) en PNG.
pub fn encode_bgra_png(
    width: u32,
    height: u32,
    stride: usize,
    bgra: &[u8],
    flip_vertical: bool,
) -> Result<Vec<u8>, DeviceCaptureError> {
    if width == 0 || height == 0 {
        return Err(DeviceCaptureError::Backend("frame webcam vide".into()));
    }
    let row = (width as usize).saturating_mul(4);
    if stride < row {
        return Err(DeviceCaptureError::Backend("stride RGB trop petit".into()));
    }
    let needed = stride.saturating_mul(height as usize);
    if bgra.len() < needed {
        return Err(DeviceCaptureError::Backend("buffer RGB trop petit".into()));
    }
    let mut img: image::RgbaImage = image::ImageBuffer::new(width, height);
    for y in 0..height {
        let src_y = if flip_vertical { height - 1 - y } else { y };
        let start = src_y as usize * stride;
        let row_bytes = &bgra[start..start + row];
        for x in 0..width as usize {
            let i = x * 4;
            let b = row_bytes[i];
            let g = row_bytes[i + 1];
            let r = row_bytes[i + 2];
            let a = row_bytes[i + 3];
            img.put_pixel(
                x as u32,
                y,
                image::Rgba([r, g, b, if a == 0 { 255 } else { a }]),
            );
        }
    }
    // Cap the long edge so mtmd/vision infer stays bounded (webcam frames can be 1080p+).
    const MAX_VISION_EDGE: u32 = 1280;
    let img = if width.max(height) > MAX_VISION_EDGE {
        let scale = MAX_VISION_EDGE as f32 / width.max(height) as f32;
        let nw = ((width as f32) * scale).round().max(1.0) as u32;
        let nh = ((height as f32) * scale).round().max(1.0) as u32;
        image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
    Ok(out)
}

#[cfg(windows)]
fn windows_camera_png(
    device: &DeviceDescriptor,
    max_bytes: u64,
) -> Result<Vec<u8>, DeviceCaptureError> {
    use std::ptr::null_mut;
    use windows::Win32::Media::MediaFoundation::{
        IMFActivate, IMFMediaType, MFCreateAttributes, MFCreateMediaType,
        MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFShutdown, MFStartup,
        MFSTARTUP_FULL, MFVideoFormat_RGB32, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
        MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE,
        MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READERF_ENDOFSTREAM,
        MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
        MFMediaType_Video,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
    };
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(mf_error)?;
        MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(mf_error)?;
        let result = (|| {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 1).map_err(mf_error)?;
            let attrs = attrs.ok_or_else(|| mf_error("attributs absents"))?;
            attrs
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(mf_error)?;
            let mut raw: *mut Option<IMFActivate> = null_mut();
            let mut count = 0u32;
            MFEnumDeviceSources(&attrs, &mut raw, &mut count).map_err(mf_error)?;
            let wanted_index = device
                .id
                .rsplit(':')
                .next()
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0);
            let mut selected = None;
            if !raw.is_null() {
                let entries = std::slice::from_raw_parts_mut(raw, count as usize);
                for (index, entry) in entries.iter_mut().enumerate() {
                    if let Some(activate) = entry.take() {
                        if index == wanted_index {
                            selected = Some(activate);
                            break;
                        }
                    }
                }
                CoTaskMemFree(Some(raw as *const _));
            }
            let activate =
                selected.ok_or_else(|| DeviceCaptureError::DeviceAbsent(device.id.clone()))?;
            let source: windows::Win32::Media::MediaFoundation::IMFMediaSource =
                activate.ActivateObject().map_err(mf_error)?;
            let mut reader_attrs = None;
            MFCreateAttributes(&mut reader_attrs, 1).map_err(mf_error)?;
            let reader_attrs = reader_attrs.ok_or_else(|| mf_error("attributs reader absents"))?;
            reader_attrs
                .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .map_err(mf_error)?;
            let reader =
                MFCreateSourceReaderFromMediaSource(&source, &reader_attrs).map_err(mf_error)?;
            let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
            reader.SetStreamSelection(stream, true).map_err(mf_error)?;
            let media_type: IMFMediaType = MFCreateMediaType().map_err(mf_error)?;
            media_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(mf_error)?;
            media_type
                .SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)
                .map_err(mf_error)?;
            reader
                .SetCurrentMediaType(stream, None, &media_type)
                .map_err(mf_error)?;
            let current = reader.GetCurrentMediaType(stream).map_err(mf_error)?;
            let packed = current.GetUINT64(&MF_MT_FRAME_SIZE).map_err(mf_error)?;
            let width = (packed >> 32) as u32;
            let height = packed as u32;
            let stride_attr = current.GetUINT32(&MF_MT_DEFAULT_STRIDE).unwrap_or(width * 4);
            let stride_i = stride_attr as i32;
            let flip = stride_i < 0;
            let stride = stride_i.unsigned_abs() as usize;
            let mut sample = None;
            for _ in 0..45 {
                let mut flags = 0u32;
                sample = None;
                reader
                    .ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))
                    .map_err(mf_error)?;
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
                if sample.is_some() {
                    break;
                }
            }
            let sample = sample.ok_or_else(|| {
                DeviceCaptureError::Backend("sample webcam Media Foundation vide".into())
            })?;
            let buffer = sample.ConvertToContiguousBuffer().map_err(mf_error)?;
            let mut ptr = null_mut();
            let mut current_len = 0u32;
            buffer
                .Lock(&mut ptr, None, Some(&mut current_len))
                .map_err(mf_error)?;
            let raw = std::slice::from_raw_parts(ptr, current_len as usize);
            let png = encode_bgra_png(width, height, stride.max(width as usize * 4), raw, flip);
            buffer.Unlock().map_err(mf_error)?;
            source.Shutdown().ok();
            let png = png?;
            if (png.len() as u64) > max_bytes {
                return Err(DeviceCaptureError::QuotaExceeded("taille".into()));
            }
            Ok(png)
        })();
        MFShutdown().ok();
        CoUninitialize();
        result
    }
}

pub fn default_backend() -> Arc<dyn DeviceCaptureBackend> {
    #[cfg(windows)]
    {
        return Arc::new(WindowsMediaFoundationBackend);
    }
    #[cfg(not(windows))]
    {
        Arc::new(UnsupportedPlatformBackend)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistentPermission {
    agent_id: String,
    device_id: String,
    kind: DeviceKind,
    mode: CaptureMode,
    cap: String,
}

#[derive(Debug)]
struct ActiveCapture {
    agent_id: String,
    device_id: String,
    cap: String,
    started: u64,
    output: PathBuf,
    stream: BackendStream,
}

pub struct DeviceCaptureManager {
    sessions_root: PathBuf,
    permissions_path: PathBuf,
    backend: Arc<dyn DeviceCaptureBackend>,
    permissions: Vec<PersistentPermission>,
    active: HashMap<String, ActiveCapture>,
    next_id: AtomicU64,
}

// serde is deliberately private to the persistence record above.
use serde::{Deserialize, Serialize};

fn absolute_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}

impl DeviceCaptureManager {
    pub fn open(sessions_root: impl Into<PathBuf>) -> Result<Self, DeviceCaptureError> {
        Self::with_backend(sessions_root, default_backend())
    }

    pub fn with_backend(
        sessions_root: impl Into<PathBuf>,
        backend: Arc<dyn DeviceCaptureBackend>,
    ) -> Result<Self, DeviceCaptureError> {
        let sessions_root = absolute_path(sessions_root.into());
        fs::create_dir_all(&sessions_root)
            .map_err(|e| DeviceCaptureError::Artifact(e.to_string()))?;
        let permissions_path = sessions_root.join("device-permissions.json");
        let permissions = match fs::read_to_string(&permissions_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(DeviceCaptureError::Artifact(e.to_string())),
        };
        Ok(Self {
            sessions_root,
            permissions_path,
            backend,
            permissions,
            active: HashMap::new(),
            next_id: AtomicU64::new(1),
        })
    }

    pub fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, DeviceCaptureError> {
        self.backend.enumerate()
    }

    pub fn backend(&self) -> &Arc<dyn DeviceCaptureBackend> {
        &self.backend
    }

    pub fn has_persistent_cap(
        &self,
        agent_id: &str,
        device_id: &str,
        kind: DeviceKind,
        mode: CaptureMode,
    ) -> bool {
        let cap = capability_for(kind, mode);
        self.permissions
            .iter()
            .any(|p| p.agent_id == agent_id && p.device_id == device_id && p.cap == cap)
    }

    pub fn persistent_permissions(
        &self,
        agent_id: Option<&str>,
    ) -> Vec<aos_proto::DevicePermissionInfo> {
        self.permissions
            .iter()
            .filter(|p| agent_id.map(|a| a == p.agent_id).unwrap_or(true))
            .map(|p| aos_proto::DevicePermissionInfo {
                agent_id: p.agent_id.clone(),
                device_id: p.device_id.clone(),
                kind: p.kind,
                mode: p.mode,
                capability: p.cap.clone(),
            })
            .collect()
    }

    pub fn grant_persistent(
        &mut self,
        agent_id: &str,
        device_id: &str,
        kind: DeviceKind,
        mode: CaptureMode,
    ) -> Result<(), DeviceCaptureError> {
        let cap = capability_for(kind, mode);
        if !self.has_persistent_cap(agent_id, device_id, kind, mode) {
            self.permissions.push(PersistentPermission {
                agent_id: agent_id.into(),
                device_id: device_id.into(),
                kind,
                mode,
                cap,
            });
            self.persist_permissions()?;
        }
        Ok(())
    }

    /// Révoque exactement le couple agent+périphérique+action et arrête ses
    /// flux avant de rendre la main.
    pub fn revoke(
        &mut self,
        agent_id: &str,
        device_id: &str,
        kind: DeviceKind,
        mode: CaptureMode,
    ) -> Result<Vec<String>, DeviceCaptureError> {
        let cap = capability_for(kind, mode);
        self.permissions
            .retain(|p| !(p.agent_id == agent_id && p.device_id == device_id && p.cap == cap));
        self.persist_permissions()?;
        let ids: Vec<String> = self
            .active
            .iter()
            .filter(|(_, c)| c.agent_id == agent_id && c.device_id == device_id && c.cap == cap)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            let _ = self.stop(id);
        }
        Ok(ids)
    }

    pub fn capture(
        &mut self,
        req: &DeviceCaptureRequest,
        policy_allowed: bool,
    ) -> Result<DeviceCaptureResponse, DeviceCaptureError> {
        validate_request(req)?;
        if !policy_allowed {
            return Err(DeviceCaptureError::InvalidRequest(
                "permission refusée par la politique".into(),
            ));
        }
        let devices = self.enumerate()?;
        let device = devices
            .into_iter()
            .find(|d| d.id == req.device_id && d.kind == req.kind)
            .ok_or_else(|| DeviceCaptureError::DeviceAbsent(req.device_id.clone()))?;
        match device.os_permission {
            OsPermissionState::Denied => return Err(DeviceCaptureError::OsPermissionDenied),
            OsPermissionState::Unknown | OsPermissionState::Granted => {}
        }
        if req.permission == CapturePermission::Always {
            self.grant_persistent(&req.agent_id, &req.device_id, req.kind, req.mode)?;
        }
        let session = safe_component(&req.session_id)?;
        let session_dir = self.sessions_root.join(&session).join("devices");
        fs::create_dir_all(&session_dir)
            .map_err(|e| DeviceCaptureError::Artifact(e.to_string()))?;
        let count = fs::read_dir(&session_dir)
            .map_err(|e| DeviceCaptureError::Artifact(e.to_string()))?
            .filter_map(Result::ok)
            .filter(|e| e.path().is_file())
            .count();
        if count >= MAX_CAPTURES_PER_SESSION {
            return Err(DeviceCaptureError::QuotaExceeded(
                "nombre de captures".into(),
            ));
        }
        let max_bytes = req
            .max_bytes
            .unwrap_or(MAX_CAPTURE_BYTES)
            .min(MAX_CAPTURE_BYTES);
        let max_duration = req
            .max_duration_ms
            .unwrap_or(MAX_CAPTURE_DURATION_MS)
            .min(MAX_CAPTURE_DURATION_MS);
        let id = format!(
            "cap-{}-{}",
            now_ms(),
            self.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let ext = match (req.kind, req.mode) {
            (DeviceKind::Camera, CaptureMode::Once) => "png",
            (DeviceKind::Microphone, CaptureMode::Once) => "pcm",
            (DeviceKind::Camera, CaptureMode::Stream) => "bin",
            (DeviceKind::Microphone, CaptureMode::Stream) => "pcm",
        };
        let output = session_dir.join(format!("{id}.{ext}"));
        // Réserve le fichier avant de lancer un flux asynchrone : la réponse
        // IPC peut donc référencer un artefact contrôlé dès l'ouverture.
        if req.mode == CaptureMode::Stream {
            File::create(&output).map_err(|e| DeviceCaptureError::Artifact(e.to_string()))?;
        }
        let root = self
            .sessions_root
            .canonicalize()
            .map_err(|e| DeviceCaptureError::Artifact(e.to_string()))?;
        let parent = output
            .parent()
            .unwrap()
            .canonicalize()
            .map_err(|e| DeviceCaptureError::Artifact(e.to_string()))?;
        if !parent.starts_with(&root) {
            return Err(DeviceCaptureError::Artifact("chemin hors session".into()));
        }
        let started = now_ms();
        let backend_result = match req.mode {
            CaptureMode::Once => {
                let result = self.backend.capture_once(&device, &output, max_bytes)?;
                if result.size_bytes > max_bytes {
                    let _ = fs::remove_file(&output);
                    return Err(DeviceCaptureError::QuotaExceeded("taille".into()));
                }
                let artifact = artifact_for(&output, &id, result.size_bytes, &result.mime_type)?;
                return Ok(DeviceCaptureResponse {
                    capture_id: id.clone(),
                    artifact,
                    metadata: CaptureMetadata {
                        capture_id: id,
                        agent_id: req.agent_id.clone(),
                        device_id: req.device_id.clone(),
                        kind: req.kind,
                        mode: req.mode,
                        started_ts_ms: started,
                        duration_ms: result.duration_ms,
                        size_bytes: result.size_bytes,
                        state: CaptureState::Completed,
                    },
                });
            }
            CaptureMode::Stream => {
                self.backend
                    .start_stream(&device, &output, max_duration, max_bytes)?
            }
        };
        let mime = backend_result.mime_type.clone();
        self.active.insert(
            id.clone(),
            ActiveCapture {
                agent_id: req.agent_id.clone(),
                device_id: req.device_id.clone(),
                cap: capability_for(req.kind, req.mode),
                started,
                output: output.clone(),
                stream: backend_result,
            },
        );
        let size = fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
        Ok(DeviceCaptureResponse {
            capture_id: id.clone(),
            artifact: artifact_for(&output, &id, size, &mime)?,
            metadata: CaptureMetadata {
                capture_id: id,
                agent_id: req.agent_id.clone(),
                device_id: req.device_id.clone(),
                kind: req.kind,
                mode: req.mode,
                started_ts_ms: started,
                duration_ms: 0,
                size_bytes: size,
                state: CaptureState::Active,
            },
        })
    }

    pub fn stop(&mut self, capture_id: &str) -> Result<(u64, u64), DeviceCaptureError> {
        let mut active = self
            .active
            .remove(capture_id)
            .ok_or_else(|| DeviceCaptureError::CaptureNotFound(capture_id.into()))?;
        active.stream.stop.store(true, Ordering::Release);
        if let Some(join) = active.stream.join.take() {
            let _ = join.join();
        }
        let size = fs::metadata(&active.output).map(|m| m.len()).unwrap_or(0);
        Ok((now_ms().saturating_sub(active.started), size))
    }

    pub fn stop_for_agent(
        &mut self,
        capture_id: &str,
        agent_id: &str,
    ) -> Result<(u64, u64), DeviceCaptureError> {
        let Some(active) = self.active.get(capture_id) else {
            return Err(DeviceCaptureError::CaptureNotFound(capture_id.into()));
        };
        if active.agent_id != agent_id {
            return Err(DeviceCaptureError::InvalidRequest(
                "capture détenue par un autre agent".into(),
            ));
        }
        self.stop(capture_id)
    }

    pub fn active_capture_ids(&mut self) -> Vec<String> {
        let finished: Vec<String> = self
            .active
            .iter()
            .filter(|(_, capture)| capture.stream.finished.load(Ordering::Acquire))
            .map(|(id, _)| id.clone())
            .collect();
        for id in finished {
            let _ = self.stop(&id);
        }
        self.active.keys().cloned().collect()
    }

    pub fn active_captures(&mut self) -> Vec<aos_proto::DeviceActiveCapture> {
        let ids = self.active_capture_ids();
        ids.into_iter()
            .filter_map(|id| {
                self.active
                    .get(&id)
                    .map(|c| aos_proto::DeviceActiveCapture {
                        capture_id: id,
                        agent_id: c.agent_id.clone(),
                        device_id: c.device_id.clone(),
                        kind: if c.cap.starts_with("device.camera") {
                            DeviceKind::Camera
                        } else {
                            DeviceKind::Microphone
                        },
                        mode: CaptureMode::Stream,
                        duration_ms: now_ms().saturating_sub(c.started),
                        size_bytes: fs::metadata(&c.output).map(|m| m.len()).unwrap_or(0),
                    })
            })
            .collect()
    }

    fn persist_permissions(&self) -> Result<(), DeviceCaptureError> {
        let raw = serde_json::to_vec_pretty(&self.permissions)
            .map_err(|e| DeviceCaptureError::Artifact(e.to_string()))?;
        let tmp = self.permissions_path.with_extension("json.tmp");
        fs::write(&tmp, raw).map_err(|e| DeviceCaptureError::Artifact(e.to_string()))?;
        fs::rename(tmp, &self.permissions_path)
            .map_err(|e| DeviceCaptureError::Artifact(e.to_string()))
    }
}

fn validate_request(req: &DeviceCaptureRequest) -> Result<(), DeviceCaptureError> {
    if req.agent_id.trim().is_empty()
        || req.device_id.trim().is_empty()
        || req.session_id.trim().is_empty()
    {
        return Err(DeviceCaptureError::InvalidRequest(
            "agent_id, device_id et session_id sont requis".into(),
        ));
    }
    if req.max_bytes == Some(0) || req.max_duration_ms == Some(0) {
        return Err(DeviceCaptureError::InvalidRequest(
            "les quotas doivent être strictement positifs".into(),
        ));
    }
    Ok(())
}

fn safe_component(value: &str) -> Result<String, DeviceCaptureError> {
    let value = value.trim();
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
    {
        return Err(DeviceCaptureError::Artifact(
            "composant de chemin invalide".into(),
        ));
    }
    Ok(value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect())
}

fn artifact_for(
    path: &Path,
    id: &str,
    size: u64,
    mime: &str,
) -> Result<DeviceArtifact, DeviceCaptureError> {
    if !path.is_file() {
        return Err(DeviceCaptureError::Artifact(
            "le backend n'a pas produit l'artefact".into(),
        ));
    }
    Ok(DeviceArtifact {
        artifact_id: id.into(),
        path: absolute_path(path.to_path_buf())
            .to_string_lossy()
            .into_owned(),
        size_bytes: size,
        mime_type: mime.into(),
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Backend déterministe injectable par les tests et la CI.
#[derive(Debug)]
pub struct FakeDeviceCaptureBackend {
    pub devices: Vec<DeviceDescriptor>,
    pub once_error: Option<DeviceCaptureError>,
    pub stream_error: Option<DeviceCaptureError>,
}

impl FakeDeviceCaptureBackend {
    pub fn new(devices: Vec<DeviceDescriptor>) -> Self {
        Self {
            devices,
            once_error: None,
            stream_error: None,
        }
    }
}

impl DeviceCaptureBackend for FakeDeviceCaptureBackend {
    fn enumerate(&self) -> Result<Vec<DeviceDescriptor>, DeviceCaptureError> {
        Ok(self.devices.clone())
    }

    fn capture_once(
        &self,
        device: &DeviceDescriptor,
        output: &Path,
        max_bytes: u64,
    ) -> Result<BackendCapture, DeviceCaptureError> {
        if let Some(e) = &self.once_error {
            return Err(e.clone());
        }
        if device.kind == DeviceKind::Camera {
            let px = [
                0u8, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
            ];
            let png = encode_bgra_png(2, 2, 8, &px, false)?;
            fs::write(output, &png).map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
            return Ok(BackendCapture {
                size_bytes: png.len() as u64,
                duration_ms: 10,
                mime_type: "image/png".into(),
            });
        }
        let n = max_bytes.min(4096).max(1) as usize;
        let mut file =
            File::create(output).map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
        file.write_all(&vec![0xA5; n])
            .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
        Ok(BackendCapture {
            size_bytes: n as u64,
            duration_ms: 10,
            mime_type: "application/octet-stream".into(),
        })
    }

    fn start_stream(
        &self,
        _device: &DeviceDescriptor,
        output: &Path,
        max_duration_ms: u64,
        max_bytes: u64,
    ) -> Result<BackendStream, DeviceCaptureError> {
        if let Some(e) = &self.stream_error {
            return Err(e.clone());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let finished_thread = finished.clone();
        let output = output.to_path_buf();
        let join = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(output)
                .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
            let mut total = 0u64;
            while !stop_thread.load(Ordering::Acquire)
                && started.elapsed().as_millis() < max_duration_ms as u128
                && total < max_bytes
            {
                let n = (1024u64).min(max_bytes - total) as usize;
                file.write_all(&vec![0x5A; n])
                    .map_err(|e| DeviceCaptureError::Backend(e.to_string()))?;
                total += n as u64;
                std::thread::sleep(Duration::from_millis(5));
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::device_capture::{CaptureMode, CapturePermission, DeviceKind};

    fn request(mode: CaptureMode, permission: CapturePermission) -> DeviceCaptureRequest {
        DeviceCaptureRequest {
            agent_id: "agent:a".into(),
            device_id: "cam-1".into(),
            kind: DeviceKind::Camera,
            mode,
            session_id: "session-1".into(),
            max_duration_ms: Some(20),
            max_bytes: Some(4096),
            permission,
        }
    }

    fn manager() -> DeviceCaptureManager {
        let root = std::env::temp_dir().join(format!("aos-device-test-{}", now_ms()));
        let backend = FakeDeviceCaptureBackend::new(vec![DeviceDescriptor {
            id: "cam-1".into(),
            name: "Test camera".into(),
            kind: DeviceKind::Camera,
            os_permission: OsPermissionState::Granted,
        }]);
        DeviceCaptureManager::with_backend(root, Arc::new(backend)).unwrap()
    }

    #[test]
    fn artifact_is_confined_and_audit_never_needs_media_bytes() {
        let mut m = manager();
        let r = m
            .capture(
                &request(CaptureMode::Once, CapturePermission::AllowOnce),
                true,
            )
            .unwrap();
        assert!(r.artifact.path.contains("devices"));
        assert!(r.artifact.path.ends_with(".png"));
        assert!(std::path::Path::new(&r.artifact.path).is_absolute());
        assert_eq!(r.artifact.mime_type, "image/png");
        assert_eq!(r.metadata.size_bytes, r.artifact.size_bytes);
        assert!(!serde_json::to_string(&r).unwrap().contains("A5"));
        let bytes = fs::read(&r.artifact.path).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn encode_bgra_png_writes_valid_signature() {
        let px = [0u8, 0, 255, 255, 255, 255, 255, 255];
        let png = encode_bgra_png(2, 1, 8, &px, false).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(png.len() > 8);
    }

    #[test]
    fn stream_stops_and_revocation_is_scoped() {
        let mut m = manager();
        let req = request(CaptureMode::Stream, CapturePermission::Always);
        let r = m.capture(&req, true).unwrap();
        assert_eq!(m.active_capture_ids(), vec![r.capture_id.clone()]);
        let stopped = m
            .revoke(&req.agent_id, &req.device_id, req.kind, req.mode)
            .unwrap();
        assert_eq!(stopped, vec![r.capture_id]);
        assert!(!m.has_persistent_cap(&req.agent_id, &req.device_id, req.kind, req.mode));
    }

    #[test]
    fn path_traversal_is_rejected() {
        let mut m = manager();
        let mut req = request(CaptureMode::Once, CapturePermission::AllowOnce);
        req.session_id = "../escape".into();
        assert!(matches!(
            m.capture(&req, true),
            Err(DeviceCaptureError::Artifact(_))
        ));
    }

    #[test]
    fn always_is_persistent_but_scoped_to_device_and_action() {
        let root = std::env::temp_dir().join(format!("aos-device-persist-{}", now_ms()));
        let backend = FakeDeviceCaptureBackend::new(vec![
            DeviceDescriptor {
                id: "cam-1".into(),
                name: "One".into(),
                kind: DeviceKind::Camera,
                os_permission: OsPermissionState::Granted,
            },
            DeviceDescriptor {
                id: "cam-2".into(),
                name: "Two".into(),
                kind: DeviceKind::Camera,
                os_permission: OsPermissionState::Granted,
            },
        ]);
        let mut m = DeviceCaptureManager::with_backend(root.clone(), Arc::new(backend)).unwrap();
        let req = request(CaptureMode::Once, CapturePermission::Always);
        m.capture(&req, true).unwrap();
        assert!(m.has_persistent_cap("agent:a", "cam-1", DeviceKind::Camera, CaptureMode::Once));
        assert!(!m.has_persistent_cap("agent:a", "cam-2", DeviceKind::Camera, CaptureMode::Once));
        assert!(!m.has_persistent_cap("agent:a", "cam-1", DeviceKind::Camera, CaptureMode::Stream));
        let m2 =
            DeviceCaptureManager::with_backend(root, Arc::new(UnsupportedPlatformBackend)).unwrap();
        assert!(m2.has_persistent_cap("agent:a", "cam-1", DeviceKind::Camera, CaptureMode::Once));
    }

    #[test]
    fn backend_and_absent_device_errors_are_kept_explicit() {
        let mut m = manager();
        let mut req = request(CaptureMode::Once, CapturePermission::AllowOnce);
        req.device_id = "missing".into();
        assert!(matches!(
            m.capture(&req, true),
            Err(DeviceCaptureError::DeviceAbsent(_))
        ));
        let root = std::env::temp_dir().join(format!("aos-device-error-{}", now_ms()));
        let mut fake = FakeDeviceCaptureBackend::new(vec![DeviceDescriptor {
            id: "cam-1".into(),
            name: "Test".into(),
            kind: DeviceKind::Camera,
            os_permission: OsPermissionState::Granted,
        }]);
        fake.once_error = Some(DeviceCaptureError::DeviceBusy);
        let mut errored = DeviceCaptureManager::with_backend(root, Arc::new(fake)).unwrap();
        assert!(matches!(
            errored.capture(
                &request(CaptureMode::Once, CapturePermission::AllowOnce),
                true
            ),
            Err(DeviceCaptureError::DeviceBusy)
        ));
    }

    #[test]
    fn stream_is_reaped_after_automatic_quota_stop() {
        let mut m = manager();
        let mut req = request(CaptureMode::Stream, CapturePermission::AllowOnce);
        req.max_bytes = Some(1);
        let r = m.capture(&req, true).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            m.active_capture_ids().is_empty(),
            "{} doit être fermé par le quota",
            r.capture_id
        );
    }
}
