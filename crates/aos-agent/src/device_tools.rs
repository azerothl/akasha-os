//! Appels IPC `device.*` pour workers et tours de salon.

use aos_ipc::BusClient;
use aos_proto::{
    CaptureMode, CapturePermission, DeviceCaptureRequest, DeviceCaptureResponse,
    DeviceCaptureStopRequest, DeviceEnumerateResponse, DeviceKind,
};
use std::path::PathBuf;

fn first_json_value(s: &str) -> Option<serde_json::Value> {
    let start = s.find('{')?;
    serde_json::Deserializer::from_str(&s[start..])
        .into_iter::<serde_json::Value>()
        .next()?
        .ok()
}

pub fn capture_png_path_from_tool_result(outcome: &str) -> Option<String> {
    let v = first_json_value(outcome)?;
    let path = v
        .pointer("/artifact/path")
        .and_then(|p| p.as_str())
        .or_else(|| v.get("path").and_then(|p| p.as_str()))?;
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        Some(canonicalize_capture_image_path(path))
    } else {
        None
    }
}

/// Make a capture artifact path loadable by modeld (absolute, under AOS_HOME if relative).
pub fn canonicalize_capture_image_path(path: &str) -> String {
    let p = PathBuf::from(path);
    if p.is_file() {
        return p.to_string_lossy().into_owned();
    }
    if let Ok(home) = std::env::var("AOS_HOME") {
        let joined = PathBuf::from(home).join(path);
        if joined.is_file() {
            return joined.to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

fn parse_capture_mode(args: &serde_json::Value) -> CaptureMode {
    match args
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("once")
        .to_ascii_lowercase()
        .as_str()
    {
        "stream" => CaptureMode::Stream,
        _ => CaptureMode::Once,
    }
}

async fn first_device_id(bus: &BusClient, kind: DeviceKind) -> Result<String, String> {
    match bus
        .call::<(), DeviceEnumerateResponse>(
            aos_proto::device_capture::intents::ENUMERATE,
            &(),
            vec![],
        )
        .await
    {
        Ok(resp) => resp
            .devices
            .into_iter()
            .find(|d| d.kind == kind)
            .map(|d| d.id)
            .ok_or_else(|| match kind {
                DeviceKind::Camera => {
                    "aucune caméra détectée — vérifie les permissions Windows Caméra".into()
                }
                DeviceKind::Microphone => {
                    "aucun microphone détecté — vérifie les permissions Windows Microphone".into()
                }
            }),
        Err(e) => Err(format!("device.enumerate err: {e}")),
    }
}

pub async fn invoke_device_tool(
    bus: &BusClient,
    agent_id: &str,
    tool: &str,
    args: &serde_json::Value,
    session_id: Option<&str>,
) -> String {
    match tool {
        "device.enumerate" => match bus
            .call::<(), DeviceEnumerateResponse>(
                aos_proto::device_capture::intents::ENUMERATE,
                &(),
                vec![],
            )
            .await
        {
            Ok(resp) if resp.devices.is_empty() => {
                "aucun périphérique caméra/micro détecté. Sous Windows, autorise Caméra et Microphone pour Akasha OS.".into()
            }
            Ok(resp) => serde_json::to_string(&resp.devices).unwrap_or_default(),
            Err(e) => format!(
                "device.enumerate err: {e}. La capture native n'est disponible que sous Windows dans Preview."
            ),
        },
        "device.camera.capture" | "device.mic.capture" => {
            let kind = if tool == "device.camera.capture" {
                DeviceKind::Camera
            } else {
                DeviceKind::Microphone
            };
            let Some(session_id) = session_id.filter(|s| !s.is_empty()) else {
                return format!("{tool} err: session_id manquant — relance depuis le chat lié");
            };
            let device_id = match args
                .get("device_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                Some(id) => id.to_string(),
                None => match first_device_id(bus, kind).await {
                    Ok(id) => id,
                    Err(e) => return e,
                },
            };
            let req = DeviceCaptureRequest {
                agent_id: format!("agent:{agent_id}"),
                device_id,
                kind,
                mode: parse_capture_mode(args),
                session_id: session_id.to_string(),
                max_duration_ms: args.get("max_duration_ms").and_then(|v| v.as_u64()),
                max_bytes: args.get("max_bytes").and_then(|v| v.as_u64()),
                permission: CapturePermission::Ask,
            };
            let intent = if kind == DeviceKind::Camera {
                aos_proto::device_capture::intents::CAMERA_CAPTURE
            } else {
                aos_proto::device_capture::intents::MIC_CAPTURE
            };
            match bus
                .call::<DeviceCaptureRequest, DeviceCaptureResponse>(intent, &req, vec![])
                .await
            {
                Ok(resp) => {
                    let json = serde_json::to_string(&resp).unwrap_or_default();
                    if kind == DeviceKind::Camera && resp.artifact.mime_type == "image/png" {
                        format!(
                            "{json}\nPNG webcam capturé. L'image est jointe au prochain tour vision — décris uniquement ce que tu vois, puis goal.complete. Interdit : tool:describe_image, inventer un bureau Windows, dire que la webcam est indisponible."
                        )
                    } else if kind == DeviceKind::Microphone {
                        format!(
                            "{json}\nCapture micro enregistrée. La transcription vocale (STT) n'est pas dans Preview — ne prétends pas entendre le contenu."
                        )
                    } else {
                        json
                    }
                }
                Err(e) => format!("{tool} err: {e}"),
            }
        }
        "device.capture.stop" => {
            let capture_id = args
                .get("capture_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if capture_id.is_empty() {
                return "device.capture.stop err: capture_id requis".into();
            }
            match bus
                .call::<DeviceCaptureStopRequest, aos_proto::DeviceCaptureStopResponse>(
                    aos_proto::device_capture::intents::CAPTURE_STOP,
                    &DeviceCaptureStopRequest {
                        agent_id: format!("agent:{agent_id}"),
                        capture_id,
                    },
                    vec![],
                )
                .await
            {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("device.capture.stop err: {e}"),
            }
        }
        other => format!("outil device inconnu: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_png_path_from_capture_json() {
        let out = r#"{"capture_id":"c1","artifact":{"artifact_id":"c1","path":"C:\\var\\s\\devices\\c1.png","size_bytes":12,"mime_type":"image/png"},"metadata":{}}
PNG webcam capturé."#;
        assert_eq!(
            capture_png_path_from_tool_result(out).as_deref(),
            Some(r"C:\var\s\devices\c1.png")
        );
        assert!(capture_png_path_from_tool_result("err: busy").is_none());
        assert!(capture_png_path_from_tool_result(
            r#"{"artifact":{"path":"clip.pcm"}}"#
        )
        .is_none());
    }

    #[test]
    fn canonicalize_keeps_existing_absolute_png() {
        let p = std::env::temp_dir().join(format!(
            "aos-cap-abs-{}.png",
            std::process::id()
        ));
        std::fs::write(&p, b"x").unwrap();
        let got = canonicalize_capture_image_path(p.to_str().unwrap());
        let _ = std::fs::remove_file(&p);
        assert_eq!(std::path::Path::new(&got), p.as_path());
    }

    #[test]
    fn canonicalize_leaves_missing_path_unchanged() {
        assert_eq!(
            canonicalize_capture_image_path("missing-capture.png"),
            "missing-capture.png"
        );
    }
}
