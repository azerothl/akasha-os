//! Contrat IPC pour les périphériques d'entrée (issue #137).
//!
//! Les réponses de capture ne contiennent jamais les octets audio/vidéo. Elles
//! contiennent uniquement une référence vers un artefact privé à la session.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub mod intents {
    pub const ENUMERATE: &str = "device.enumerate";
    pub const CAMERA_CAPTURE: &str = "device.camera.capture";
    pub const MIC_CAPTURE: &str = "device.mic.capture";
    pub const CAPTURE_STOP: &str = "device.capture.stop";
    pub const CAPTURE_ACTIVE: &str = "device.capture.active";
}

/// Périphérique d'entrée supporté par la première slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Camera,
    Microphone,
}

impl DeviceKind {
    #[allow(non_upper_case_globals)]
    pub const Mic: Self = Self::Microphone;

    pub const fn capability_prefix(self) -> &'static str {
        match self {
            Self::Camera => "device.camera",
            Self::Microphone => "device.mic",
        }
    }
}

/// Capture ponctuelle ou flux contrôlé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    Once,
    Stream,
}

impl CaptureMode {
    /// Alias lisible pour les clients qui nomment explicitement le mode
    /// « one-shot ».
    #[allow(non_upper_case_globals)]
    pub const OneShot: Self = Self::Once;

    pub const fn capability_suffix(self) -> &'static str {
        match self {
            Self::Once => "capture",
            Self::Stream => "stream",
        }
    }
}

/// Choix explicite présenté par l'UI après une confirmation Akasha.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapturePermission {
    /// Demander à `policy_gate` (valeur sûre pour les appels d'agents).
    #[default]
    Ask,
    /// Autorisation limitée à cette requête.
    AllowOnce,
    /// Autorisation persistante pour cet agent et ce périphérique précis.
    Always,
}

/// État de la permission système observé par le backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OsPermissionState {
    #[default]
    Unknown,
    Granted,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceDescriptor {
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    #[serde(default)]
    pub os_permission: OsPermissionState,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceEnumerateResponse {
    pub devices: Vec<DeviceDescriptor>,
}

/// Requête commune aux intents caméra/microphone.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceCaptureRequest {
    pub agent_id: String,
    pub device_id: String,
    pub kind: DeviceKind,
    pub mode: CaptureMode,
    pub session_id: String,
    /// Durée demandée en millisecondes. Le service applique toujours son
    /// plafond et celui de la politique locale.
    #[serde(default)]
    pub max_duration_ms: Option<u64>,
    #[serde(default)]
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub permission: CapturePermission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptureState {
    Active,
    Completed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceArtifact {
    pub artifact_id: String,
    /// Chemin contrôlé par le service, jamais fourni par l'appelant.
    pub path: String,
    pub size_bytes: u64,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaptureMetadata {
    pub capture_id: String,
    pub agent_id: String,
    pub device_id: String,
    pub kind: DeviceKind,
    pub mode: CaptureMode,
    pub started_ts_ms: u64,
    pub duration_ms: u64,
    pub size_bytes: u64,
    pub state: CaptureState,
}

pub type CaptureId = String;
pub type DeviceCaptureMetadata = CaptureMetadata;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceCaptureResponse {
    pub capture_id: String,
    pub artifact: DeviceArtifact,
    pub metadata: CaptureMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceCaptureStopRequest {
    pub agent_id: String,
    pub capture_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceCaptureStopResponse {
    pub capture_id: String,
    pub stopped: bool,
    pub duration_ms: u64,
}

/// Alias courts conservant un vocabulaire naturel côté clients IPC.
pub type CaptureRequest = DeviceCaptureRequest;
pub type CaptureResponse = DeviceCaptureResponse;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DevicePermissionRevokeRequest {
    pub agent_id: String,
    pub device_id: String,
    pub kind: DeviceKind,
    pub mode: CaptureMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DevicePermissionInfo {
    pub agent_id: String,
    pub device_id: String,
    pub kind: DeviceKind,
    pub mode: CaptureMode,
    pub capability: String,
}

pub type DeviceCapability = DevicePermissionInfo;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceActiveCapture {
    pub capture_id: String,
    pub agent_id: String,
    pub device_id: String,
    pub kind: DeviceKind,
    pub mode: CaptureMode,
    pub duration_ms: u64,
    pub size_bytes: u64,
}

/// Capabilities exactes utilisées par `device.camera.capture`, etc.
pub fn capability_for(kind: DeviceKind, mode: CaptureMode) -> String {
    format!("{}.{}", kind.capability_prefix(), mode.capability_suffix())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_shape_is_stable_and_defaults_permission_to_ask() {
        let req: DeviceCaptureRequest = serde_json::from_str(
            r#"{"agent_id":"a","device_id":"cam-1","kind":"camera","mode":"once","session_id":"s"}"#,
        )
        .unwrap();
        assert_eq!(req.permission, CapturePermission::Ask);
        assert_eq!(
            capability_for(DeviceKind::Camera, CaptureMode::Stream),
            "device.camera.stream"
        );
        let bytes = serde_json::to_vec(&req).unwrap();
        assert!(bytes.windows(b"media".len()).all(|w| w != b"media"));
    }
}
