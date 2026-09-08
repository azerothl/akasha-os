//! Demande d'accès hors sandbox via `fs.host.access` (issue #157).

use aos_ipc::BusClient;
use aos_proto::host_folder::intents;
use aos_proto::{
    HostFolderAccessRequest, HostFolderAccessResponse, HostFolderOperation, HostFolderPermission,
};
use crate::storage_path::{is_disallowed_storage_path, ROOM_HOST_PATH_DISALLOWED};
use serde_json::Value;

pub async fn try_host_folder_tool(
    bus: &BusClient,
    agent_id: &str,
    tool: &str,
    args: &Value,
    session_id: Option<&str>,
) -> Option<String> {
    let path = storage_path_arg(tool, args)?;
    if !is_disallowed_storage_path(path) {
        return None;
    }
    let session_id = session_id.filter(|s| !s.is_empty())?;
    let (operation, content, format) = match tool {
        "fs.read" => (HostFolderOperation::Read, None, None),
        "fs.write" => (
            HostFolderOperation::Write,
            args.get("content").and_then(|v| v.as_str()),
            None,
        ),
        "fs.list" => (HostFolderOperation::List, None, None),
        "files.generate" => (
            HostFolderOperation::Generate,
            args.get("content").and_then(|v| v.as_str()),
            args.get("format").and_then(|v| v.as_str()),
        ),
        _ => return None,
    };
    let req = HostFolderAccessRequest {
        agent_id: format!("agent:{agent_id}"),
        session_id: session_id.to_string(),
        path: path.to_string(),
        operation,
        content: content.map(|s| s.to_string()),
        format: format.map(|s| s.to_string()),
        permission: HostFolderPermission::Ask,
        trace_id: format!("host-folder-{}", std::process::id()),
    };
    match bus
        .call::<HostFolderAccessRequest, HostFolderAccessResponse>(intents::ACCESS, &req, vec![])
        .await
    {
        Ok(resp) if resp.ok => {
            if let Some(content) = resp.content {
                Some(content)
            } else if let Some(entries) = resp.entries {
                Some(serde_json::to_string(&entries).unwrap_or_default())
            } else {
                Some("ok".into())
            }
        }
        Ok(_) | Err(_) => Some(ROOM_HOST_PATH_DISALLOWED.to_string()),
    }
}

fn storage_path_arg<'a>(tool: &str, args: &'a Value) -> Option<&'a str> {
    let key = match tool {
        "fs.read" | "fs.write" | "files.generate" => "path",
        "fs.list" => "prefix",
        _ => return None,
    };
    args.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disallowed_path_triggers_host_folder_flow() {
        let args = serde_json::json!({ "path": "e:/test/test/out.md" });
        assert!(storage_path_arg("fs.write", &args).is_some());
        assert!(is_disallowed_storage_path("e:/test/test/out.md"));
    }

    #[test]
    fn sandbox_paths_skip_host_folder_flow() {
        assert!(!is_disallowed_storage_path("/documents/notes/a.md"));
        let args = serde_json::json!({ "path": "/downloads/x.md" });
        assert!(storage_path_arg("fs.write", &args).is_some());
        assert!(!is_disallowed_storage_path("/downloads/x.md"));
    }
}
