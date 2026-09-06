//! Contrat IPC pour l'accès USB opt-in (issue #137, slice 3).
//!
//! Les réponses d'I/O ne contiennent jamais de dumps complets dans l'audit.
//! Les octets transitent uniquement dans les payloads IPC read/write.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod intents {
    pub const ENUMERATE: &str = "device.usb.enumerate";
    pub const OPEN: &str = "device.usb.open";
    pub const READ: &str = "device.usb.read";
    pub const WRITE: &str = "device.usb.write";
    pub const CLOSE: &str = "device.usb.close";
    pub const HANDLE_ACTIVE: &str = "device.usb.handle.active";
}

/// Famille de périphérique USB exposée aux agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsbDeviceClass {
    Serial,
    Hid,
    Generic,
}

impl UsbDeviceClass {
    pub const fn capability_prefix(self) -> &'static str {
        match self {
            Self::Serial | Self::Hid | Self::Generic => "device.usb",
        }
    }
}

/// Choix explicite présenté par l'UI après confirmation Akasha.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsbPermission {
    #[default]
    Ask,
    AllowOnce,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbDeviceDescriptor {
    pub id: String,
    pub name: String,
    pub class: UsbDeviceClass,
    #[serde(default)]
    pub vendor_id: Option<u16>,
    #[serde(default)]
    pub product_id: Option<u16>,
    /// Chemin ou port hôte opaque (ex. `COM3`, `\\?\USB#...`). Non audité en entier.
    #[serde(default)]
    pub path_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbEnumerateResponse {
    pub devices: Vec<UsbDeviceDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbOpenRequest {
    pub agent_id: String,
    pub device_id: String,
    pub session_id: String,
    #[serde(default)]
    pub permission: UsbPermission,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbOpenResponse {
    pub handle_id: String,
    pub device_id: String,
    pub class: UsbDeviceClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbReadRequest {
    pub agent_id: String,
    pub handle_id: String,
    #[serde(default)]
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbReadResponse {
    pub handle_id: String,
    pub bytes_read: u64,
    /// Octets lus, encodés base64 standard (pas d'hex dump dans l'audit).
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbWriteRequest {
    pub agent_id: String,
    pub handle_id: String,
    /// Octets à écrire, encodés base64 standard.
    pub data_base64: String,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbWriteResponse {
    pub handle_id: String,
    pub bytes_written: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbCloseRequest {
    pub agent_id: String,
    pub handle_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbCloseResponse {
    pub handle_id: String,
    pub closed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbPermissionRevokeRequest {
    pub agent_id: String,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbPermissionInfo {
    pub agent_id: String,
    pub device_id: String,
    pub capability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UsbActiveHandle {
    pub handle_id: String,
    pub agent_id: String,
    pub device_id: String,
    pub class: UsbDeviceClass,
    pub opened_ts_ms: u64,
}

/// Capabilité unique pour open/read/write USB.
pub fn usb_io_capability() -> String {
    "device.usb.io".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_shape_defaults_permission_to_ask() {
        let req: UsbOpenRequest = serde_json::from_str(
            r#"{"agent_id":"a","device_id":"usb-1","session_id":"s"}"#,
        )
        .unwrap();
        assert_eq!(req.permission, UsbPermission::Ask);
        assert_eq!(usb_io_capability(), "device.usb.io");
    }
}
