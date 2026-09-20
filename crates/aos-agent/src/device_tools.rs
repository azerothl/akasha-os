//! Appels IPC `device.*` pour workers et tours de salon.

use aos_ipc::BusClient;
use aos_proto::{
    CaptureMode, CapturePermission, DeviceCaptureRequest, DeviceCaptureResponse,
    DeviceCaptureStopRequest, DeviceEnumerateResponse, DeviceKind, UsbCloseRequest,
    UsbDeviceDescriptor, UsbEnumerateResponse, UsbOpenRequest, UsbPermission, UsbReadRequest,
    UsbWriteRequest,
};
use std::path::PathBuf;

fn usb_devices_for_agent(devices: &[UsbDeviceDescriptor]) -> String {
    let slim: Vec<serde_json::Value> = devices
        .iter()
        .map(|d| {
            serde_json::json!({
                "id": d.id,
                "name": d.name,
                "class": d.class,
                "vendor_id": d.vendor_id,
                "product_id": d.product_id,
            })
        })
        .collect();
    serde_json::to_string(&slim).unwrap_or_else(|_| "[]".into())
}

fn first_json_value(s: &str) -> Option<serde_json::Value> {
    let start = s.find('{')?;
    serde_json::Deserializer::from_str(&s[start..])
        .into_iter::<serde_json::Value>()
        .next()?
        .ok()
}

/// Pending revisions must be inspected together, source first and candidate second.
pub fn illustration_image_refs(outcome: &str) -> Vec<String> {
    if let Some(v) = first_json_value(outcome) {
        let doc = v.get("doc").unwrap_or(&v);
        if let Some(run) = doc.get("image_run") {
            if run.get("candidate_selected").is_none_or(|v| v.is_null()) {
                if let (Some(source), Some(candidate)) = (
                    run.get("source_png").and_then(|v| v.as_str()),
                    run.get("candidate_png").and_then(|v| v.as_str()),
                ) {
                    if [source, candidate].iter().all(|p| p.to_ascii_lowercase().ends_with(".png")) {
                        return vec![canonicalize_capture_image_path(source), canonicalize_capture_image_path(candidate)];
                    }
                }
            }
        }
    }
    capture_png_path_from_tool_result(outcome).into_iter().collect()
}

/// Only a successfully decoded, actually running image job enters runtime wait.
pub fn illustration_running_run(outcome: &str) -> Option<String> {
    let value = first_json_value(outcome)?;
    let doc = value.get("doc").unwrap_or(&value);
    let run = doc.get("image_run")?;
    if run.get("status")?.as_str()? != "running" { return None; }
    run.get("id")?.as_str().filter(|id| !id.is_empty()).map(str::to_owned)
}

pub fn illustration_run_is_pending(doc: &aos_proto::IllustrationDoc, id: &str) -> bool {
    doc.image_run.as_ref().is_some_and(|run| run.id == id
        && run.status == aos_proto::IllustrationImageStatus::Running)
}

/// Describe only the image attachment state, never infer visual quality from JSON.
pub fn illustration_completion_error(doc: &aos_proto::IllustrationDoc) -> Option<String> {
    use aos_proto::IllustrationImageStatus;
    let Some(run) = &doc.image_run else {
        return Some("Illustration : aucune génération lancée. Appelle illust.generate_image avec construction et frame_subject ; un brief seul n'est pas un rendu.".into());
    };
    match run.status {
        IllustrationImageStatus::Running => Some("Illustration : génération encore en cours. Suis les passes avec illust.get avant de terminer.".into()),
        IllustrationImageStatus::Failed => Some(format!("Illustration : génération en échec : {}. Corrige la cause ou utilise goal.fail, sans annoncer de rendu réussi.", run.error.as_deref().unwrap_or("erreur inconnue"))),
        IllustrationImageStatus::NeedsReview if run.candidate_png.is_some() && run.candidate_selected.is_none() =>
            Some("Illustration : retouche non résolue. Compare original et proposition avant illust.resolve_image.".into()),
        IllustrationImageStatus::NeedsReview if doc.last_png.is_none() => Some("Illustration : aucun PNG sélectionné. Le résultat n'est pas terminé.".into()),
        IllustrationImageStatus::NeedsReview => None,
    }
}

pub fn illustration_vision_feedback(image_count: usize, attached: bool) -> Option<&'static str> {
    if image_count == 0 {
        return None;
    }
    if !attached {
        return Some("[runtime Illustration] Les PNG existent mais ne sont PAS joints à ce tour : le modèle actif ne dispose pas de vision utilisable. Tu n'as pas vu ces images. Ne prétends pas avoir vérifié l'anatomie, la pose, les détails ou la fidélité au brief. Ne décide pas d'une retouche ni de son acceptation sur la seule base du JSON ou d'un score structurel. Signale que la vérification visuelle reste à faire par l'utilisateur ou un modèle vision. Tu peux continuer à suivre les passes avec illust.get sans les relancer.");
    }
    Some(if image_count == 2 {
        "[runtime Illustration] Deux PNG sont joints, dans cet ordre : original conservé, puis proposition de retouche. Compare réellement les deux avec le brief utilisateur avant illust.resolve_image. Une perte de détail demandé ou une régression anatomique interdit d'accepter la proposition comme amélioration. La fin du calcul ne vaut pas validation artistique."
    } else {
        "[runtime Illustration] Le PNG de l'aperçu courant est joint. Inspecte son contenu par rapport au brief utilisateur ; ne confonds pas une passe intermédiaire avec le rendu final. needs_review indique seulement la fin du calcul, pas une validation artistique."
    })
}

pub fn capture_png_path_from_tool_result(outcome: &str) -> Option<String> {
    let v = first_json_value(outcome)?;
    let path = v
        .pointer("/artifact/path")
        .and_then(|p| p.as_str())
        .or_else(|| v.get("path").and_then(|p| p.as_str()))
        .or_else(|| v.pointer("/doc/last_png").and_then(|p| p.as_str()))
        .or_else(|| v.get("last_png").and_then(|p| p.as_str()))?;
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
        "device.usb.enumerate" => match bus
            .call::<(), UsbEnumerateResponse>(
                aos_proto::device_usb::intents::ENUMERATE,
                &(),
                vec![],
            )
            .await
        {
            Ok(resp) if resp.devices.is_empty() => {
                "aucun périphérique USB détecté. Branche un adaptateur série USB.".into()
            }
            Ok(resp) => usb_devices_for_agent(&resp.devices),
            Err(e) => format!("device.usb.enumerate err: {e}."),
        },
        "device.usb.open" => {
            let Some(session_id) = session_id.filter(|s| !s.is_empty()) else {
                return "device.usb.open err: session_id manquant — relance depuis le chat lié".into();
            };
            let device_id = match args
                .get("device_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                Some(id) => id.to_string(),
                None => return "device.usb.open err: device_id requis".into(),
            };
            let req = UsbOpenRequest {
                agent_id: format!("agent:{agent_id}"),
                device_id,
                session_id: session_id.to_string(),
                permission: UsbPermission::Ask,
            };
            match bus
                .call::<UsbOpenRequest, aos_proto::UsbOpenResponse>(
                    aos_proto::device_usb::intents::OPEN,
                    &req,
                    vec![],
                )
                .await
            {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("device.usb.open err: {e}"),
            }
        }
        "device.usb.read" => {
            let handle_id = args
                .get("handle_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if handle_id.is_empty() {
                return "device.usb.read err: handle_id requis".into();
            }
            let req = UsbReadRequest {
                agent_id: format!("agent:{agent_id}"),
                handle_id,
                max_bytes: args.get("max_bytes").and_then(|v| v.as_u64()),
                timeout_ms: args.get("timeout_ms").and_then(|v| v.as_u64()),
            };
            match bus
                .call::<UsbReadRequest, aos_proto::UsbReadResponse>(
                    aos_proto::device_usb::intents::READ,
                    &req,
                    vec![],
                )
                .await
            {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("device.usb.read err: {e}"),
            }
        }
        "device.usb.write" => {
            let handle_id = args
                .get("handle_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let data_base64 = args
                .get("data_base64")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if handle_id.is_empty() || data_base64.is_empty() {
                return "device.usb.write err: handle_id et data_base64 requis".into();
            }
            let req = UsbWriteRequest {
                agent_id: format!("agent:{agent_id}"),
                handle_id,
                data_base64,
                timeout_ms: args.get("timeout_ms").and_then(|v| v.as_u64()),
            };
            match bus
                .call::<UsbWriteRequest, aos_proto::UsbWriteResponse>(
                    aos_proto::device_usb::intents::WRITE,
                    &req,
                    vec![],
                )
                .await
            {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("device.usb.write err: {e}"),
            }
        }
        "device.usb.close" => {
            let handle_id = args
                .get("handle_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if handle_id.is_empty() {
                return "device.usb.close err: handle_id requis".into();
            }
            match bus
                .call::<UsbCloseRequest, aos_proto::UsbCloseResponse>(
                    aos_proto::device_usb::intents::CLOSE,
                    &UsbCloseRequest {
                        agent_id: format!("agent:{agent_id}"),
                        handle_id,
                    },
                    vec![],
                )
                .await
            {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("device.usb.close err: {e}"),
            }
        }
        other => format!("outil device inconnu: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::device_usb::{UsbDeviceClass, UsbDeviceDescriptor};

    #[test]
    fn usb_devices_for_agent_omits_path_hint() {
        let devices = vec![UsbDeviceDescriptor {
            id: "linux:Serial:/dev/ttyUSB0".into(),
            name: "CP2102".into(),
            class: UsbDeviceClass::Serial,
            vendor_id: Some(0x10c4),
            product_id: Some(0xea60),
            path_hint: Some("/dev/ttyUSB0".into()),
        }];
        let json = usb_devices_for_agent(&devices);
        assert!(!json.contains("path_hint"));
        assert!(json.contains(r#""name":"CP2102"#));
        assert!(json.contains("linux:Serial"));
        assert!(!json.contains(r#""name":"CP2102 (/dev"#));
    }

    #[test]
    fn extracts_png_path_from_capture_json() {
        let out = r#"{"capture_id":"c1","artifact":{"artifact_id":"c1","path":"C:\\var\\s\\devices\\c1.png","size_bytes":12,"mime_type":"image/png"},"metadata":{}}
PNG webcam capturé."#;
        assert_eq!(
            capture_png_path_from_tool_result(out).as_deref(),
            Some(r"C:\var\s\devices\c1.png")
        );
        assert!(capture_png_path_from_tool_result("err: busy").is_none());
        assert!(capture_png_path_from_tool_result(r#"{"artifact":{"path":"clip.pcm"}}"#).is_none());
    }

    #[test]
    fn illustration_vision_compares_pending_revision_in_order() {
        let mut doc = serde_json::json!({"last_png":"/downloads/source.png","image_run":{
            "source_png":"/downloads/source.png","candidate_png":"/downloads/candidate.png",
            "candidate_selected":null}});
        let expected = vec!["/downloads/source.png", "/downloads/candidate.png"];
        assert_eq!(illustration_image_refs(&doc.to_string()), expected);
        assert_eq!(illustration_image_refs(&serde_json::json!({"doc":doc.clone()}).to_string()), expected);
        doc["image_run"]["candidate_selected"] = serde_json::json!(false);
        assert_eq!(illustration_image_refs(&doc.to_string()), vec!["/downloads/source.png"]);
        doc["image_run"]["candidate_selected"] = serde_json::json!(true);
        doc["last_png"] = serde_json::json!("/downloads/candidate.png");
        assert_eq!(illustration_image_refs(&doc.to_string()), vec!["/downloads/candidate.png"]);
    }

    #[test]
    fn illustration_feedback_distinguishes_unseen_preview_and_comparison() {
        assert!(illustration_vision_feedback(0, false).is_none());
        assert!(illustration_vision_feedback(0, true).is_none());
        for count in [1, 2] {
            let unseen = illustration_vision_feedback(count, false).unwrap();
            assert!(unseen.contains("PAS joints"));
            assert!(unseen.contains("Tu n'as pas vu"));
            assert!(!unseen.contains("Deux PNG sont joints"));
        }
        assert!(illustration_vision_feedback(1, true).unwrap().contains("aperçu courant"));
        assert!(illustration_vision_feedback(2, true).unwrap().contains("original conservé, puis proposition"));
    }

    #[test]
    fn illustration_completion_requires_a_finished_selected_image() {
        let mut doc = aos_proto::IllustrationDoc::default();
        assert!(illustration_completion_error(&doc).unwrap().contains("aucune génération"));
        doc.image_run = Some(serde_json::from_value(serde_json::json!({
            "id":"r", "source_revision":1, "phase":"final", "status":"running"
        })).unwrap());
        doc.last_png = Some("intermediate.png".into());
        assert!(illustration_completion_error(&doc).unwrap().contains("en cours"));
        doc.image_run.as_mut().unwrap().status = aos_proto::IllustrationImageStatus::Failed;
        assert!(illustration_completion_error(&doc).unwrap().contains("en échec"));
        doc.image_run.as_mut().unwrap().status = aos_proto::IllustrationImageStatus::NeedsReview;
        doc.last_png = None;
        assert!(illustration_completion_error(&doc).is_some());
        doc.last_png = Some("final.png".into());
        assert!(illustration_completion_error(&doc).is_none());
        doc.image_run.as_mut().unwrap().candidate_png = Some("candidate.png".into());
        assert!(illustration_completion_error(&doc).unwrap().contains("non résolue"));
        doc.image_run.as_mut().unwrap().candidate_selected = Some(false);
        assert!(illustration_completion_error(&doc).is_none());
    }

    #[test]
    fn illustration_wait_tracks_only_the_requested_running_job() {
        assert!(illustration_running_run("ERREUR: failed").is_none());
        assert!(illustration_running_run(r#"{"image_run":{"id":"r","status":"failed"}}"#).is_none());
        assert!(illustration_running_run(r#"{"image_run":{"id":"","status":"running"}}"#).is_none());
        assert_eq!(illustration_running_run(r#"{"doc":{"image_run":{"id":"r","status":"running"}}}"#).as_deref(), Some("r"));
        let mut doc = aos_proto::IllustrationDoc::default();
        assert!(!illustration_run_is_pending(&doc, "r"));
        doc.image_run = Some(serde_json::from_value(serde_json::json!({
            "id":"r", "source_revision":1, "phase":"skeleton", "status":"running"
        })).unwrap());
        assert!(illustration_run_is_pending(&doc, "r"));
        assert!(!illustration_run_is_pending(&doc, "replaced"));
        doc.image_run.as_mut().unwrap().status = aos_proto::IllustrationImageStatus::NeedsReview;
        assert!(!illustration_run_is_pending(&doc, "r"));
        doc.image_run.as_mut().unwrap().status = aos_proto::IllustrationImageStatus::Failed;
        assert!(!illustration_run_is_pending(&doc, "r"));
    }

    #[test]
    fn illustration_vision_uses_current_preview_not_history() {
        let out = r#"{"doc":{"last_png":"/downloads/illustration/current.png","pass_previews":[["skeleton",1,"/downloads/illustration/old.png"]]}}"#;
        assert_eq!(capture_png_path_from_tool_result(out).as_deref(), Some("/downloads/illustration/current.png"));
        assert_eq!(capture_png_path_from_tool_result(r#"{"last_png":"/downloads/illustration/final.png"}"#).as_deref(), Some("/downloads/illustration/final.png"));
        assert!(capture_png_path_from_tool_result(r#"{"doc":{"last_png":null,"pass_previews":[["final",1,"old.png"]]}}"#).is_none());
    }

    #[test]
    fn canonicalize_keeps_existing_absolute_png() {
        let p = std::env::temp_dir().join(format!("aos-cap-abs-{}.png", std::process::id()));
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
