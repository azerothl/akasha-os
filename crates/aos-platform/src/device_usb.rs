//! Accès USB opt-in : énumération et I/O contraint (issue #137, slice 3).
//!
//! Le manager ne laisse jamais le backend choisir des chemins arbitraires. Les
//! handles ouverts sont limités par agent et par quota global.

use aos_proto::device_usb::{
    usb_io_capability, UsbActiveHandle, UsbCloseRequest, UsbCloseResponse, UsbDeviceClass,
    UsbDeviceDescriptor, UsbOpenRequest, UsbOpenResponse, UsbPermission,
    UsbPermissionInfo, UsbReadRequest, UsbReadResponse, UsbWriteRequest, UsbWriteResponse,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const MAX_USB_READ_BYTES: u64 = 4096;
pub const MAX_USB_WRITE_BYTES: u64 = 4096;
pub const MAX_USB_IO_TIMEOUT_MS: u64 = 5_000;
pub const MAX_USB_HANDLES_PER_AGENT: usize = 8;
pub const MAX_USB_HANDLES_TOTAL: usize = 32;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum UsbIoError {
    #[error("plateforme non supportée pour l'accès USB")]
    UnsupportedPlatform,
    #[error("périphérique absent: {0}")]
    DeviceAbsent(String),
    #[error("permission refusée pour le périphérique")]
    OsPermissionDenied,
    #[error("périphérique occupé")]
    DeviceBusy,
    #[error("quota atteint: {0}")]
    QuotaExceeded(String),
    #[error("handle inconnu: {0}")]
    HandleNotFound(String),
    #[error("requête invalide: {0}")]
    InvalidRequest(String),
    #[error("backend: {0}")]
    Backend(String),
    #[error("timeout I/O")]
    IoTimeout,
}

/// Handle opaque côté backend.
pub trait UsbDeviceHandle: Send {
    fn read(&mut self, max_bytes: usize, timeout: Duration) -> Result<Vec<u8>, UsbIoError>;
    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize, UsbIoError>;
    fn close(&mut self) -> Result<(), UsbIoError>;
}

pub trait UsbIoBackend: Send + Sync {
    fn enumerate(&self) -> Result<Vec<UsbDeviceDescriptor>, UsbIoError>;
    fn open(&self, device: &UsbDeviceDescriptor) -> Result<Box<dyn UsbDeviceHandle>, UsbIoError>;
}

#[derive(Debug, Default)]
pub struct UnsupportedPlatformUsbBackend;

impl UsbIoBackend for UnsupportedPlatformUsbBackend {
    fn enumerate(&self) -> Result<Vec<UsbDeviceDescriptor>, UsbIoError> {
        Err(UsbIoError::UnsupportedPlatform)
    }

    fn open(&self, _device: &UsbDeviceDescriptor) -> Result<Box<dyn UsbDeviceHandle>, UsbIoError> {
        Err(UsbIoError::UnsupportedPlatform)
    }
}

pub fn default_usb_backend() -> Arc<dyn UsbIoBackend> {
    #[cfg(windows)]
    {
        return Arc::new(WindowsUsbBackend);
    }
    #[cfg(not(windows))]
    {
        Arc::new(UnsupportedPlatformUsbBackend)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistentUsbPermission {
    agent_id: String,
    device_id: String,
    cap: String,
}

struct ActiveUsbHandle {
    agent_id: String,
    device_id: String,
    class: UsbDeviceClass,
    opened_ts_ms: u64,
    handle: Box<dyn UsbDeviceHandle>,
}

pub struct UsbIoManager {
    permissions_path: PathBuf,
    backend: Arc<dyn UsbIoBackend>,
    permissions: Vec<PersistentUsbPermission>,
    active: HashMap<String, ActiveUsbHandle>,
    next_id: AtomicU64,
}

impl UsbIoManager {
    pub fn open(sessions_root: impl Into<PathBuf>) -> Result<Self, UsbIoError> {
        Self::with_backend(sessions_root, default_usb_backend())
    }

    pub fn with_backend(
        sessions_root: impl Into<PathBuf>,
        backend: Arc<dyn UsbIoBackend>,
    ) -> Result<Self, UsbIoError> {
        let sessions_root = absolute_path(sessions_root.into());
        fs::create_dir_all(&sessions_root).map_err(|e| UsbIoError::Backend(e.to_string()))?;
        let permissions_path = sessions_root.join("usb-permissions.json");
        let permissions = match fs::read_to_string(&permissions_path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(UsbIoError::Backend(e.to_string())),
        };
        Ok(Self {
            permissions_path,
            backend,
            permissions,
            active: HashMap::new(),
            next_id: AtomicU64::new(1),
        })
    }

    pub fn enumerate(&self) -> Result<Vec<UsbDeviceDescriptor>, UsbIoError> {
        self.backend.enumerate()
    }

    pub fn backend(&self) -> &Arc<dyn UsbIoBackend> {
        &self.backend
    }

    pub fn has_persistent_cap(&self, agent_id: &str, device_id: &str) -> bool {
        let cap = usb_io_capability();
        self.permissions
            .iter()
            .any(|p| p.agent_id == agent_id && p.device_id == device_id && p.cap == cap)
    }

    pub fn persistent_permissions(&self, agent_id: Option<&str>) -> Vec<UsbPermissionInfo> {
        self.permissions
            .iter()
            .filter(|p| agent_id.map(|a| a == p.agent_id).unwrap_or(true))
            .map(|p| UsbPermissionInfo {
                agent_id: p.agent_id.clone(),
                device_id: p.device_id.clone(),
                capability: p.cap.clone(),
            })
            .collect()
    }

    pub fn grant_persistent(
        &mut self,
        agent_id: &str,
        device_id: &str,
    ) -> Result<(), UsbIoError> {
        let cap = usb_io_capability();
        if !self.has_persistent_cap(agent_id, device_id) {
            self.permissions.push(PersistentUsbPermission {
                agent_id: agent_id.into(),
                device_id: device_id.into(),
                cap,
            });
            self.persist_permissions()?;
        }
        Ok(())
    }

    pub fn revoke(
        &mut self,
        agent_id: &str,
        device_id: &str,
    ) -> Result<Vec<String>, UsbIoError> {
        let cap = usb_io_capability();
        self.permissions
            .retain(|p| !(p.agent_id == agent_id && p.device_id == device_id && p.cap == cap));
        self.persist_permissions()?;
        let ids: Vec<String> = self
            .active
            .iter()
            .filter(|(_, h)| h.agent_id == agent_id && h.device_id == device_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            let _ = self.close_handle(&UsbCloseRequest {
                agent_id: agent_id.into(),
                handle_id: id.clone(),
            });
        }
        Ok(ids)
    }

    pub fn open_device(
        &mut self,
        req: &UsbOpenRequest,
        policy_allowed: bool,
    ) -> Result<UsbOpenResponse, UsbIoError> {
        validate_open(req)?;
        if !policy_allowed {
            return Err(UsbIoError::InvalidRequest(
                "permission refusée par la politique".into(),
            ));
        }
        if req.permission == UsbPermission::Always {
            self.grant_persistent(&req.agent_id, &req.device_id)?;
        }
        let devices = self.enumerate()?;
        let device = devices
            .into_iter()
            .find(|d| d.id == req.device_id)
            .ok_or_else(|| UsbIoError::DeviceAbsent(req.device_id.clone()))?;
        let agent_handles = self
            .active
            .values()
            .filter(|h| h.agent_id == req.agent_id)
            .count();
        if agent_handles >= MAX_USB_HANDLES_PER_AGENT {
            return Err(UsbIoError::QuotaExceeded("handles par agent".into()));
        }
        if self.active.len() >= MAX_USB_HANDLES_TOTAL {
            return Err(UsbIoError::QuotaExceeded("handles globaux".into()));
        }
        let handle = self.backend.open(&device)?;
        let handle_id = format!(
            "usb-{}-{}",
            now_ms(),
            self.next_id.fetch_add(1, Ordering::Relaxed)
        );
        self.active.insert(
            handle_id.clone(),
            ActiveUsbHandle {
                agent_id: req.agent_id.clone(),
                device_id: req.device_id.clone(),
                class: device.class,
                opened_ts_ms: now_ms(),
                handle,
            },
        );
        Ok(UsbOpenResponse {
            handle_id,
            device_id: req.device_id.clone(),
            class: device.class,
        })
    }

    pub fn read(&mut self, req: &UsbReadRequest) -> Result<UsbReadResponse, UsbIoError> {
        validate_agent_handle(req.agent_id.as_str(), req.handle_id.as_str())?;
        let max_bytes = req
            .max_bytes
            .unwrap_or(MAX_USB_READ_BYTES)
            .min(MAX_USB_READ_BYTES) as usize;
        let timeout_ms = req
            .timeout_ms
            .unwrap_or(MAX_USB_IO_TIMEOUT_MS)
            .min(MAX_USB_IO_TIMEOUT_MS);
        let active = self
            .active
            .get_mut(&req.handle_id)
            .ok_or_else(|| UsbIoError::HandleNotFound(req.handle_id.clone()))?;
        if active.agent_id != req.agent_id {
            return Err(UsbIoError::InvalidRequest(
                "handle détenu par un autre agent".into(),
            ));
        }
        let data = active
            .handle
            .read(max_bytes, Duration::from_millis(timeout_ms))?;
        Ok(UsbReadResponse {
            handle_id: req.handle_id.clone(),
            bytes_read: data.len() as u64,
            data_base64: B64.encode(&data),
        })
    }

    pub fn write(&mut self, req: &UsbWriteRequest) -> Result<UsbWriteResponse, UsbIoError> {
        validate_agent_handle(req.agent_id.as_str(), req.handle_id.as_str())?;
        let data = B64
            .decode(req.data_base64.as_bytes())
            .map_err(|e| UsbIoError::InvalidRequest(format!("base64 invalide: {e}")))?;
        if data.len() as u64 > MAX_USB_WRITE_BYTES {
            return Err(UsbIoError::QuotaExceeded("écriture".into()));
        }
        let timeout_ms = req
            .timeout_ms
            .unwrap_or(MAX_USB_IO_TIMEOUT_MS)
            .min(MAX_USB_IO_TIMEOUT_MS);
        let active = self
            .active
            .get_mut(&req.handle_id)
            .ok_or_else(|| UsbIoError::HandleNotFound(req.handle_id.clone()))?;
        if active.agent_id != req.agent_id {
            return Err(UsbIoError::InvalidRequest(
                "handle détenu par un autre agent".into(),
            ));
        }
        let written = active
            .handle
            .write(&data, Duration::from_millis(timeout_ms))?;
        Ok(UsbWriteResponse {
            handle_id: req.handle_id.clone(),
            bytes_written: written as u64,
        })
    }

    pub fn close_handle(&mut self, req: &UsbCloseRequest) -> Result<UsbCloseResponse, UsbIoError> {
        validate_agent_handle(req.agent_id.as_str(), req.handle_id.as_str())?;
        let mut active = self
            .active
            .remove(&req.handle_id)
            .ok_or_else(|| UsbIoError::HandleNotFound(req.handle_id.clone()))?;
        if active.agent_id != req.agent_id {
            return Err(UsbIoError::InvalidRequest(
                "handle détenu par un autre agent".into(),
            ));
        }
        active.handle.close()?;
        Ok(UsbCloseResponse {
            handle_id: req.handle_id.clone(),
            closed: true,
        })
    }

    pub fn active_handles(&self) -> Vec<UsbActiveHandle> {
        self.active
            .iter()
            .map(|(id, h)| UsbActiveHandle {
                handle_id: id.clone(),
                agent_id: h.agent_id.clone(),
                device_id: h.device_id.clone(),
                class: h.class,
                opened_ts_ms: h.opened_ts_ms,
            })
            .collect()
    }

    fn persist_permissions(&self) -> Result<(), UsbIoError> {
        let raw = serde_json::to_vec_pretty(&self.permissions)
            .map_err(|e| UsbIoError::Backend(e.to_string()))?;
        let tmp = self.permissions_path.with_extension("json.tmp");
        fs::write(&tmp, raw).map_err(|e| UsbIoError::Backend(e.to_string()))?;
        fs::rename(tmp, &self.permissions_path)
            .map_err(|e| UsbIoError::Backend(e.to_string()))
    }
}

fn validate_open(req: &UsbOpenRequest) -> Result<(), UsbIoError> {
    if req.agent_id.trim().is_empty()
        || req.device_id.trim().is_empty()
        || req.session_id.trim().is_empty()
    {
        return Err(UsbIoError::InvalidRequest(
            "agent_id, device_id et session_id sont requis".into(),
        ));
    }
    let session = req.session_id.trim();
    if session.is_empty()
        || session == "."
        || session == ".."
        || session.contains('/')
        || session.contains('\\')
    {
        return Err(UsbIoError::InvalidRequest("session_id invalide".into()));
    }
    Ok(())
}

fn validate_agent_handle(agent_id: &str, handle_id: &str) -> Result<(), UsbIoError> {
    if agent_id.trim().is_empty() || handle_id.trim().is_empty() {
        return Err(UsbIoError::InvalidRequest(
            "agent_id et handle_id sont requis".into(),
        ));
    }
    Ok(())
}

fn absolute_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// Windows backend (SetupAPI enumeration + COM serial I/O)
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod windows_backend {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::Devices::Communication::{
        GetCommState, SetCommState, SetCommTimeouts, COMMTIMEOUTS, DCB, NOPARITY, ONESTOPBIT,
    };
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW, DIGCF_ALLCLASSES,
        DIGCF_PRESENT, SP_DEVINFO_DATA, SPDRP_FRIENDLYNAME,
    };
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        ReadFile, WriteFile,
    };
    use windows::Win32::System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_READ, RegCloseKey, RegEnumValueW, RegOpenKeyExW,
    };

    fn win_error(msg: impl std::fmt::Display) -> UsbIoError {
        UsbIoError::Backend(msg.to_string())
    }

    pub struct WindowsUsbBackend;

    struct SerialHandle {
        handle: HANDLE,
    }

    // Exclusive COM-port HANDLE owned by this wrapper.
    unsafe impl Send for SerialHandle {}

    impl UsbDeviceHandle for SerialHandle {
        fn read(&mut self, max_bytes: usize, timeout: Duration) -> Result<Vec<u8>, UsbIoError> {
            set_comm_timeouts(self.handle, timeout)?;
            let mut buf = vec![0u8; max_bytes];
            let mut read = 0u32;
            unsafe { ReadFile(self.handle, Some(&mut buf), Some(&mut read), None) }
                .map_err(win_error)?;
            buf.truncate(read as usize);
            Ok(buf)
        }

        fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize, UsbIoError> {
            set_comm_timeouts(self.handle, timeout)?;
            let mut written = 0u32;
            unsafe { WriteFile(self.handle, Some(data), Some(&mut written), None) }
                .map_err(win_error)?;
            Ok(written as usize)
        }

        fn close(&mut self) -> Result<(), UsbIoError> {
            if !self.handle.is_invalid() {
                let _ = unsafe { CloseHandle(self.handle) };
                self.handle = INVALID_HANDLE_VALUE;
            }
            Ok(())
        }
    }

    fn set_comm_timeouts(handle: HANDLE, timeout: Duration) -> Result<(), UsbIoError> {
        let ms = timeout.as_millis().min(u32::MAX as u128) as u32;
        let timeouts = COMMTIMEOUTS {
            ReadIntervalTimeout: ms,
            ReadTotalTimeoutMultiplier: 0,
            ReadTotalTimeoutConstant: ms,
            WriteTotalTimeoutMultiplier: 0,
            WriteTotalTimeoutConstant: ms,
        };
        unsafe { SetCommTimeouts(handle, &timeouts) }.map_err(win_error)
    }

    fn open_serial_port(port: &str) -> Result<Box<dyn UsbDeviceHandle>, UsbIoError> {
        let path = if port.starts_with("\\\\.\\") {
            port.to_string()
        } else {
            format!("\\\\.\\{}", port)
        };
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let handle = unsafe {
            CreateFileW(
                PCWSTR(wide.as_ptr()),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .map_err(|_| UsbIoError::DeviceBusy)?;
        if handle == INVALID_HANDLE_VALUE {
            return Err(UsbIoError::OsPermissionDenied);
        }
        let mut dcb = DCB::default();
        dcb.DCBlength = std::mem::size_of::<DCB>() as u32;
        unsafe { GetCommState(handle, &mut dcb) }.map_err(win_error)?;
        dcb.BaudRate = 115200;
        dcb.ByteSize = 8;
        dcb.Parity = NOPARITY;
        dcb.StopBits = ONESTOPBIT;
        unsafe { SetCommState(handle, &dcb) }.map_err(win_error)?;
        Ok(Box::new(SerialHandle { handle }))
    }

    fn enumerate_com_ports() -> Vec<UsbDeviceDescriptor> {
        let mut devices = Vec::new();
        let key_path = "HARDWARE\\DEVICEMAP\\SERIALCOMM";
        let wide: Vec<u16> = key_path.encode_utf16().chain([0]).collect();
        let mut hkey = HKEY::default();
        if unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(wide.as_ptr()),
                0,
                KEY_READ,
                &mut hkey,
            )
        } != ERROR_SUCCESS
        {
            return devices;
        }
        let mut index = 0u32;
        loop {
            let mut name = [0u16; 256];
            let mut name_len = name.len() as u32;
            let mut value = [0u16; 64];
            let mut value_len = (value.len() * 2) as u32;
            let mut kind = 0u32;
            let err = unsafe {
                RegEnumValueW(
                    hkey,
                    index,
                    windows::core::PWSTR(name.as_mut_ptr()),
                    &mut name_len,
                    None,
                    Some(&mut kind),
                    Some(value.as_mut_ptr() as *mut u8),
                    Some(&mut value_len),
                )
            };
            if err != ERROR_SUCCESS {
                break;
            }
            index += 1;
            let port = String::from_utf16_lossy(&value[..value_len as usize / 2])
                .trim_end_matches('\0')
                .to_string();
            if port.is_empty() {
                continue;
            }
            let id = format!("win:Serial:{}", port);
            devices.push(UsbDeviceDescriptor {
                id,
                name: format!("USB Serial ({port})"),
                class: UsbDeviceClass::Serial,
                vendor_id: None,
                product_id: None,
                path_hint: Some(port),
            });
        }
        let _ = unsafe { RegCloseKey(hkey) };
        devices
    }

    fn enumerate_usb_devices() -> Vec<UsbDeviceDescriptor> {
        let mut devices = Vec::new();
        let Ok(info) = (unsafe {
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_PRESENT | DIGCF_ALLCLASSES)
        }) else {
            return devices;
        };
        if info.is_invalid() {
            return devices;
        }
        let mut index = 0u32;
        loop {
            let mut data = SP_DEVINFO_DATA::default();
            data.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
            if unsafe { SetupDiEnumDeviceInfo(info, index, &mut data) }.is_err() {
                break;
            }
            index += 1;
            let mut instance_id = [0u16; 512];
            if unsafe { SetupDiGetDeviceInstanceIdW(info, &data, Some(&mut instance_id), None) }
                .is_err()
            {
                continue;
            }
            let instance = String::from_utf16_lossy(
                &instance_id[..instance_id.iter().position(|&c| c == 0).unwrap_or(0)],
            );
            if !instance.to_ascii_uppercase().contains("USB") {
                continue;
            }
            let mut desc_buf = [0u16; 256];
            let mut required = 0u32;
            let named = {
                let prop_bytes = unsafe {
                    std::slice::from_raw_parts_mut(
                        desc_buf.as_mut_ptr() as *mut u8,
                        desc_buf.len() * 2,
                    )
                };
                unsafe {
                    SetupDiGetDeviceRegistryPropertyW(
                        info,
                        &data,
                        SPDRP_FRIENDLYNAME,
                        None,
                        Some(prop_bytes),
                        Some(&mut required),
                    )
                }
                .is_ok()
            };
            let name = if named {
                String::from_utf16_lossy(
                    &desc_buf[..desc_buf.iter().position(|&c| c == 0).unwrap_or(0)],
                )
            } else {
                instance.clone()
            };
            let (vid, pid) = parse_vid_pid(&instance);
            let id = format!(
                "win:Usb:{:04x}:{:04x}:{}",
                vid.unwrap_or(0),
                pid.unwrap_or(0),
                index
            );
            if devices.iter().any(|d| d.id == id) {
                continue;
            }
            devices.push(UsbDeviceDescriptor {
                id,
                name,
                class: UsbDeviceClass::Generic,
                vendor_id: vid,
                product_id: pid,
                path_hint: Some(truncate_hint(&instance)),
            });
        }
        let _ = unsafe { SetupDiDestroyDeviceInfoList(info) };
        devices
    }

    fn parse_vid_pid(instance: &str) -> (Option<u16>, Option<u16>) {
        let upper = instance.to_ascii_uppercase();
        let vid = upper
            .split("VID_")
            .nth(1)
            .and_then(|s| u16::from_str_radix(s.get(..4).unwrap_or(""), 16).ok());
        let pid = upper
            .split("PID_")
            .nth(1)
            .and_then(|s| u16::from_str_radix(s.get(..4).unwrap_or(""), 16).ok());
        (vid, pid)
    }

    fn truncate_hint(instance: &str) -> String {
        if instance.len() > 64 {
            format!("{}…", &instance[..61])
        } else {
            instance.to_string()
        }
    }

    impl UsbIoBackend for WindowsUsbBackend {
        fn enumerate(&self) -> Result<Vec<UsbDeviceDescriptor>, UsbIoError> {
            let mut devices = enumerate_com_ports();
            for d in enumerate_usb_devices() {
                if !devices.iter().any(|x| x.id == d.id) {
                    devices.push(d);
                }
            }
            Ok(devices)
        }

        fn open(&self, device: &UsbDeviceDescriptor) -> Result<Box<dyn UsbDeviceHandle>, UsbIoError> {
            match device.class {
                UsbDeviceClass::Serial => {
                    let port = device
                        .path_hint
                        .as_deref()
                        .ok_or_else(|| UsbIoError::InvalidRequest("port série manquant".into()))?;
                    open_serial_port(port)
                }
                UsbDeviceClass::Hid | UsbDeviceClass::Generic => Err(UsbIoError::InvalidRequest(
                    "ouverture I/O réservée aux ports série USB dans cette tranche".into(),
                )),
            }
        }
    }
}

#[cfg(windows)]
use windows_backend::WindowsUsbBackend;

// ---------------------------------------------------------------------------
// Fake backend for CI
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct FakeUsbIoBackend {
    pub devices: Vec<UsbDeviceDescriptor>,
    pub open_error: Option<UsbIoError>,
    pub read_payload: Vec<u8>,
}

impl FakeUsbIoBackend {
    pub fn new(devices: Vec<UsbDeviceDescriptor>) -> Self {
        Self {
            devices,
            open_error: None,
            read_payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
        }
    }
}

struct FakeHandle {
    buffer: Arc<Mutex<Vec<u8>>>,
    read_payload: Vec<u8>,
}

impl UsbDeviceHandle for FakeHandle {
    fn read(&mut self, max_bytes: usize, _timeout: Duration) -> Result<Vec<u8>, UsbIoError> {
        let n = max_bytes.min(self.read_payload.len());
        Ok(self.read_payload[..n].to_vec())
    }

    fn write(&mut self, data: &[u8], _timeout: Duration) -> Result<usize, UsbIoError> {
        self.buffer.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }

    fn close(&mut self) -> Result<(), UsbIoError> {
        Ok(())
    }
}

impl UsbIoBackend for FakeUsbIoBackend {
    fn enumerate(&self) -> Result<Vec<UsbDeviceDescriptor>, UsbIoError> {
        Ok(self.devices.clone())
    }

    fn open(&self, _device: &UsbDeviceDescriptor) -> Result<Box<dyn UsbDeviceHandle>, UsbIoError> {
        if let Some(e) = &self.open_error {
            return Err(e.clone());
        }
        Ok(Box::new(FakeHandle {
            buffer: Arc::new(Mutex::new(Vec::new())),
            read_payload: self.read_payload.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::device_usb::UsbPermission;

    fn sample_device() -> UsbDeviceDescriptor {
        UsbDeviceDescriptor {
            id: "fake:Serial:COM1".into(),
            name: "Fake USB Serial".into(),
            class: UsbDeviceClass::Serial,
            vendor_id: Some(0x1234),
            product_id: Some(0x5678),
            path_hint: Some("COM1".into()),
        }
    }

    fn manager() -> UsbIoManager {
        let root = std::env::temp_dir().join(format!("aos-usb-test-{}", now_ms()));
        let backend = FakeUsbIoBackend::new(vec![sample_device()]);
        UsbIoManager::with_backend(root, Arc::new(backend)).unwrap()
    }

    fn open_req(permission: UsbPermission) -> UsbOpenRequest {
        UsbOpenRequest {
            agent_id: "agent:a".into(),
            device_id: "fake:Serial:COM1".into(),
            session_id: "session-1".into(),
            permission,
        }
    }

    #[test]
    fn enumerate_and_roundtrip_io() {
        let mut m = manager();
        let devices = m.enumerate().unwrap();
        assert_eq!(devices.len(), 1);
        let opened = m.open_device(&open_req(UsbPermission::AllowOnce), true).unwrap();
        let read = m
            .read(&UsbReadRequest {
                agent_id: "agent:a".into(),
                handle_id: opened.handle_id.clone(),
                max_bytes: Some(4),
                timeout_ms: None,
            })
            .unwrap();
        assert_eq!(read.bytes_read, 4);
        assert!(!read.data_base64.is_empty());
        let write = m
            .write(&UsbWriteRequest {
                agent_id: "agent:a".into(),
                handle_id: opened.handle_id.clone(),
                data_base64: B64.encode([1, 2, 3]),
                timeout_ms: None,
            })
            .unwrap();
        assert_eq!(write.bytes_written, 3);
        let closed = m
            .close_handle(&UsbCloseRequest {
                agent_id: "agent:a".into(),
                handle_id: opened.handle_id,
            })
            .unwrap();
        assert!(closed.closed);
    }

    #[test]
    fn always_is_persistent_but_scoped_to_device() {
        let root = std::env::temp_dir().join(format!("aos-usb-persist-{}", now_ms()));
        let backend = FakeUsbIoBackend::new(vec![
            sample_device(),
            UsbDeviceDescriptor {
                id: "fake:Serial:COM2".into(),
                name: "Two".into(),
                class: UsbDeviceClass::Serial,
                vendor_id: None,
                product_id: None,
                path_hint: Some("COM2".into()),
            },
        ]);
        let mut m = UsbIoManager::with_backend(root.clone(), Arc::new(backend)).unwrap();
        m.open_device(&open_req(UsbPermission::Always), true).unwrap();
        assert!(m.has_persistent_cap("agent:a", "fake:Serial:COM1"));
        assert!(!m.has_persistent_cap("agent:a", "fake:Serial:COM2"));
        let m2 = UsbIoManager::with_backend(
            root,
            Arc::new(UnsupportedPlatformUsbBackend),
        )
        .unwrap();
        assert!(m2.has_persistent_cap("agent:a", "fake:Serial:COM1"));
    }

    #[test]
    fn revoke_closes_handles() {
        let mut m = manager();
        let opened = m.open_device(&open_req(UsbPermission::Always), true).unwrap();
        let stopped = m.revoke("agent:a", "fake:Serial:COM1").unwrap();
        assert_eq!(stopped, vec![opened.handle_id]);
        assert!(!m.has_persistent_cap("agent:a", "fake:Serial:COM1"));
    }

    #[test]
    fn unsupported_platform_backend_fails_closed() {
        let root = std::env::temp_dir().join(format!("aos-usb-unsupported-{}", now_ms()));
        let m = UsbIoManager::with_backend(root, Arc::new(UnsupportedPlatformUsbBackend)).unwrap();
        assert!(matches!(
            m.enumerate(),
            Err(UsbIoError::UnsupportedPlatform)
        ));
    }

    #[test]
    fn audit_never_needs_raw_payload() {
        let mut m = manager();
        let opened = m.open_device(&open_req(UsbPermission::AllowOnce), true).unwrap();
        let read = m
            .read(&UsbReadRequest {
                agent_id: "agent:a".into(),
                handle_id: opened.handle_id,
                max_bytes: Some(4),
                timeout_ms: None,
            })
            .unwrap();
        let json = serde_json::to_string(&read).unwrap();
        assert!(!json.contains("deadbeef"));
    }
}
