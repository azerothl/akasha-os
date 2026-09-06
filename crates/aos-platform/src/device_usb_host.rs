//! Backends Linux/macOS pour l'accès USB série (issue #137 slice 4).
//!
//! - Énumération et I/O série via [serialport](https://crates.io/crates/serialport)
//!   (ttyUSB/ttyACM sur Linux, cu.* sur macOS).

use crate::device_usb::{UsbDeviceHandle, UsbIoBackend, UsbIoError};
use aos_proto::device_usb::{UsbDeviceClass, UsbDeviceDescriptor};
use serialport::SerialPort;
use std::io::{Read, Write};
use std::time::Duration;

const DEFAULT_BAUD: u32 = 115200;

#[derive(Debug, Default)]
pub struct HostUsbBackend;

fn platform_tag() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn map_host_error(error: impl std::fmt::Display) -> UsbIoError {
    let msg = error.to_string().to_lowercase();
    if msg.contains("permission")
        || msg.contains("denied")
        || msg.contains("not authorized")
        || msg.contains("authorization")
        || msg.contains("access denied")
        || msg.contains("operation not permitted")
    {
        UsbIoError::OsPermissionDenied
    } else if msg.contains("busy") || msg.contains("in use") || msg.contains("resource busy") {
        UsbIoError::DeviceBusy
    } else if msg.contains("timed out") || msg.contains("timeout") {
        UsbIoError::IoTimeout
    } else if msg.contains("not found") || msg.contains("no such device") || msg.contains("no such file") {
        UsbIoError::DeviceAbsent("périphérique".into())
    } else {
        UsbIoError::Backend(error.to_string())
    }
}

struct SerialHandle {
    port: Box<dyn SerialPort>,
}

impl UsbDeviceHandle for SerialHandle {
    fn read(&mut self, max_bytes: usize, timeout: Duration) -> Result<Vec<u8>, UsbIoError> {
        self.port.set_timeout(timeout).map_err(map_host_error)?;
        let mut buf = vec![0u8; max_bytes];
        match self.port.read(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                Ok(buf)
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                Ok(Vec::new())
            }
            Err(e) => Err(map_host_error(e)),
        }
    }

    fn write(&mut self, data: &[u8], timeout: Duration) -> Result<usize, UsbIoError> {
        self.port.set_timeout(timeout).map_err(map_host_error)?;
        self.port.write(data).map_err(map_host_error)
    }

    fn close(&mut self) -> Result<(), UsbIoError> {
        Ok(())
    }
}

fn open_serial_port(path: &str) -> Result<Box<dyn UsbDeviceHandle>, UsbIoError> {
    let port = serialport::new(path, DEFAULT_BAUD)
        .timeout(Duration::from_millis(100))
        .open()
        .map_err(map_host_error)?;
    Ok(Box::new(SerialHandle { port }))
}

fn serial_device_name(info: &serialport::SerialPortInfo) -> String {
    if let serialport::SerialPortType::UsbPort(usb) = &info.port_type {
        if let Some(product) = usb.product.as_deref().filter(|s| !s.is_empty()) {
            return product.to_string();
        }
        if let Some(mfr) = usb.manufacturer.as_deref().filter(|s| !s.is_empty()) {
            return mfr.to_string();
        }
    }
    "USB Serial".to_string()
}

impl UsbIoBackend for HostUsbBackend {
    fn enumerate(&self) -> Result<Vec<UsbDeviceDescriptor>, UsbIoError> {
        let ports = serialport::available_ports().map_err(map_host_error)?;
        let mut devices = Vec::with_capacity(ports.len());
        for info in ports {
            let path = info.port_name.clone();
            let id = format!("{}:Serial:{}", platform_tag(), path);
            let (vendor_id, product_id) = match &info.port_type {
                serialport::SerialPortType::UsbPort(usb) => (Some(usb.vid), Some(usb.pid)),
                _ => (None, None),
            };
            devices.push(UsbDeviceDescriptor {
                id,
                name: serial_device_name(&info),
                class: UsbDeviceClass::Serial,
                vendor_id,
                product_id,
                path_hint: Some(path),
            });
        }
        Ok(devices)
    }

    fn open(&self, device: &UsbDeviceDescriptor) -> Result<Box<dyn UsbDeviceHandle>, UsbIoError> {
        match device.class {
            UsbDeviceClass::Serial => {
                let path = device
                    .path_hint
                    .as_deref()
                    .ok_or_else(|| UsbIoError::InvalidRequest("port série manquant".into()))?;
                open_serial_port(path)
            }
            UsbDeviceClass::Hid | UsbDeviceClass::Generic => Err(UsbIoError::InvalidRequest(
                "ouverture I/O réservée aux ports série USB dans cette tranche".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_backend_is_not_unsupported_on_host() {
        let backend = crate::device_usb::default_usb_backend();
        let result = backend.enumerate();
        assert!(!matches!(result, Err(UsbIoError::UnsupportedPlatform)));
    }

    #[test]
    fn permission_error_mapping() {
        let err = map_host_error("Permission denied: /dev/ttyUSB0");
        assert_eq!(err, UsbIoError::OsPermissionDenied);
        let err = map_host_error("device or resource busy");
        assert_eq!(err, UsbIoError::DeviceBusy);
        let err = map_host_error("Operation timed out");
        assert_eq!(err, UsbIoError::IoTimeout);
    }

    #[test]
    fn serial_device_name_omits_host_path() {
        let info = serialport::SerialPortInfo {
            port_name: "/dev/ttyUSB0".into(),
            port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
                vid: 0x10c4,
                pid: 0xea60,
                serial_number: None,
                manufacturer: Some("Silicon Labs".into()),
                product: Some("CP2102 USB to UART Bridge Controller".into()),
            }),
        };
        let name = serial_device_name(&info);
        assert_eq!(name, "CP2102 USB to UART Bridge Controller");
        assert!(!name.contains("/dev/"));
        assert!(!name.contains("ttyUSB"));
    }

    #[test]
    fn serial_device_name_falls_back_without_usb_metadata() {
        let info = serialport::SerialPortInfo {
            port_name: "/dev/cu.usbserial-1410".into(),
            port_type: serialport::SerialPortType::Unknown,
        };
        let name = serial_device_name(&info);
        assert_eq!(name, "USB Serial");
        assert!(!name.contains("/dev/"));
        assert!(!name.contains("cu."));
    }

    #[test]
    fn device_id_format() {
        let backend = HostUsbBackend;
        let devices = backend.enumerate().unwrap_or_default();
        for device in devices {
            let prefix = format!("{}:Serial:", platform_tag());
            assert!(
                device.id.starts_with(&prefix),
                "unexpected id format: {}",
                device.id
            );
            assert_eq!(device.class, UsbDeviceClass::Serial);
            assert!(device.path_hint.is_some());
        }
    }
}
