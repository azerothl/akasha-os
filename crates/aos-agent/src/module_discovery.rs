//! Runtime discovery of WASM module tools (`module.list` + `module.describe`).
//!
//! Shared by worker, salon, and scheduler-launched agents (issue #149, lot 3).

use aos_ipc::BusClient;
use aos_proto::{ModuleIdRequest, ModuleInfo};
use std::collections::HashSet;

use crate::skills::SkillDoc;
use crate::tools::{ToolBackend, ToolDesc};

/// Skill names that require an installed module before their tools are advertised.
pub fn skill_required_module(skill_name: &str) -> Option<&'static str> {
    match skill_name.trim().to_ascii_lowercase().as_str() {
        "tasks" => Some(aos_proto::tasks_contract::MODULE_NAME),
        _ => None,
    }
}

/// User-facing hint when a skill is active but its module is not installed.
pub fn module_install_hint(module: &str) -> String {
    format!(
        "Le module `{module}` n'est pas installé. Propose à l'utilisateur de l'installer \
         (`module.catalogue.install` ou `module.install`) — n'invoque pas les outils \
         `{module}.*` implicitement."
    )
}

/// Installed module names that are active (present, not quarantined).
pub fn active_module_names(modules: &[ModuleInfo]) -> HashSet<String> {
    modules
        .iter()
        .filter(|m| !m.quarantined && !m.tools.is_empty())
        .map(|m| m.name.clone())
        .collect()
}

/// Whether a module is installed and active.
pub fn is_module_active(modules: &[ModuleInfo], name: &str) -> bool {
    modules
        .iter()
        .any(|m| m.name == name && !m.quarantined && !m.tools.is_empty())
}

/// Build tool descriptors from an already-fetched `module.list` snapshot.
pub fn discover_module_tools_from_list(
    modules: &[ModuleInfo],
    describe: impl Fn(&str) -> Option<serde_json::Value>,
) -> Vec<ToolDesc> {
    let mut out = Vec::new();
    for info in modules {
        if info.quarantined || info.tools.is_empty() {
            continue;
        }
        for tool in &info.tools {
            out.push(ToolDesc {
                name: tool.clone(),
                description: format!("outil module {}", info.name),
                input_schema: serde_json::json!({"type": "object"}),
                backend: ToolBackend::Module,
                required_caps: vec![format!("tool.invoke:{}", info.name)],
            });
        }
        if let Some(desc) = describe(&info.name) {
            enrich_from_describe(&mut out, &desc);
        }
    }
    out
}

async fn describe_module(bus: &BusClient, module: &str) -> Result<serde_json::Value, String> {
    bus.call::<ModuleIdRequest, serde_json::Value>(
        "module.describe",
        &ModuleIdRequest {
            module: module.to_string(),
        },
        vec![],
    )
    .await
    .map_err(|e| e.to_string())
}

/// Discover tools from installed active modules via the platform bus.
pub async fn discover_module_tools(bus: &BusClient) -> Vec<ToolDesc> {
    let list = bus
        .call::<(), Vec<ModuleInfo>>("module.list", &(), vec![])
        .await
        .unwrap_or_default();
    let mut out = Vec::new();
    for info in &list {
        if info.quarantined || info.tools.is_empty() {
            continue;
        }
        for tool in &info.tools {
            out.push(ToolDesc {
                name: tool.clone(),
                description: format!("outil module {}", info.name),
                input_schema: serde_json::json!({"type": "object"}),
                backend: ToolBackend::Module,
                required_caps: vec![format!("tool.invoke:{}", info.name)],
            });
        }
        if let Ok(desc) = describe_module(bus, &info.name).await {
            enrich_from_describe(&mut out, &desc);
        }
    }
    out
}

fn enrich_from_describe(out: &mut [ToolDesc], desc: &serde_json::Value) {
    if let Some(tools) = desc
        .get("manifest")
        .and_then(|m| m.get("tools"))
        .and_then(|t| t.as_array())
    {
        for t in tools {
            let name = t
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            if let Some(existing) = out.iter_mut().find(|x| x.name == name) {
                if let Some(d) = t.get("description").and_then(|v| v.as_str()) {
                    existing.description = d.to_string();
                }
                if let Some(schema) = t.get("input_schema") {
                    existing.input_schema = schema.clone();
                }
            }
        }
    }
}

/// Prefixes (`notes`, `tasks`, …) exported by discovered module tools.
pub fn discovered_module_prefixes(discovered: &[ToolDesc]) -> HashSet<String> {
    discovered
        .iter()
        .filter(|t| t.backend == ToolBackend::Module)
        .filter_map(|t| t.name.split('.').next().map(str::to_string))
        .collect()
}

/// Merge skill-declared tools, skipping module-bound tools when the module is absent.
pub fn merge_skill_tools_for_modules(
    selected_tools: &[String],
    skills: &[SkillDoc],
    installed_modules: &HashSet<String>,
) -> Vec<String> {
    let mut out = selected_tools.to_vec();
    for s in skills {
        let required = skill_required_module(&s.name);
        for t in &s.tools {
            if let Some(module) = required {
                if t.starts_with(&format!("{module}.")) && !installed_modules.contains(module) {
                    continue;
                }
            }
            if !out.contains(t) {
                out.push(t.clone());
            }
        }
    }
    out
}

/// Install hints for active skills whose module dependency is missing.
pub fn missing_module_hints(
    skills: &[SkillDoc],
    installed_modules: &HashSet<String>,
) -> Vec<String> {
    let mut hints = Vec::new();
    for s in skills {
        if let Some(module) = skill_required_module(&s.name) {
            if !installed_modules.contains(module) {
                hints.push(module_install_hint(module));
            }
        }
    }
    hints
}

/// Refusal when a tool was requested but is no longer in the active catalog.
/// Never embeds the technical tool id — model-facing only, may surface in salon bubbles.
pub fn tool_unavailable_message(_tool: &str, reason: &str) -> String {
    format!(
        "ERREUR outil: indisponible ({reason}). \
         Le catalogue modules a peut-être changé — ne simule pas un succès."
    )
}

/// Whether `tool` is listed in the current filtered catalog.
pub fn tool_in_catalog(tool: &str, catalog: &[ToolDesc]) -> bool {
    catalog.iter().any(|t| t.name == tool)
}

/// Whether a module-prefix tool may use the generic module fallback path.
pub fn module_fallback_allowed(tool: &str, discovered: &[ToolDesc]) -> bool {
    let prefix = tool.split('.').next().unwrap_or("");
    discovered
        .iter()
        .any(|t| t.backend == ToolBackend::Module && t.name.starts_with(&format!("{prefix}.")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::tasks_contract::{INVOKE_CAP, TOOL_IDS};

    fn sample_tasks_module() -> ModuleInfo {
        ModuleInfo {
            name: "tasks".into(),
            version: "1.0.0".into(),
            granted_caps: vec![INVOKE_CAP.into()],
            tools: TOOL_IDS.iter().map(|s| s.to_string()).collect(),
            quarantined: false,
            ui_mode: Some("declarative_ui".into()),
            ui_title: Some("Tasks".into()),
        }
    }

    fn sample_tasks_describe() -> serde_json::Value {
        serde_json::json!({
            "manifest": {
                "tools": [
                    {"name": "tasks.create", "description": "Create a task", "input_schema": {"type": "object"}},
                    {"name": "tasks.list", "description": "List tasks", "input_schema": {"type": "object"}},
                    {"name": "tasks.update", "description": "Update a task", "input_schema": {"type": "object"}},
                    {"name": "tasks.complete", "description": "Complete a task", "input_schema": {"type": "object"}}
                ]
            }
        })
    }

    #[test]
    fn tool_unavailable_message_omits_tool_id() {
        let msg = tool_unavailable_message("tasks.list", "absent du catalogue modules actif");
        assert!(!msg.contains("tasks.list"), "{msg}");
        assert!(msg.contains("indisponible"), "{msg}");
    }

    #[test]
    fn discovers_tasks_tools_from_module_list() {
        let tools = discover_module_tools_from_list(&[sample_tasks_module()], |_| {
            Some(sample_tasks_describe())
        });
        for id in TOOL_IDS {
            let tool = tools
                .iter()
                .find(|t| t.name == *id)
                .unwrap_or_else(|| panic!("missing {id}"));
            assert_eq!(tool.required_caps, vec![INVOKE_CAP.to_string()]);
            assert!(!tool.description.contains("outil module"));
        }
    }

    #[test]
    fn skips_quarantined_modules() {
        let mut m = sample_tasks_module();
        m.quarantined = true;
        let tools = discover_module_tools_from_list(&[m], |_| Some(sample_tasks_describe()));
        assert!(tools.is_empty());
    }

    #[test]
    fn merge_skill_tools_skips_tasks_when_module_absent() {
        let skill = SkillDoc {
            name: "tasks".into(),
            description: String::new(),
            when_to_use: String::new(),
            tools: vec!["tasks.create".into(), "goal.complete".into()],
            required_caps: vec![],
            body: String::new(),
            path: std::path::PathBuf::from("skills/tasks/SKILL.md"),
        };
        let merged = merge_skill_tools_for_modules(&[], &[skill], &HashSet::new());
        assert!(!merged.iter().any(|t| t.starts_with("tasks.")));
        assert!(merged.iter().any(|t| t == "goal.complete"));
    }

    #[test]
    fn merge_skill_tools_keeps_tasks_when_module_present() {
        let skill = SkillDoc {
            name: "tasks".into(),
            description: String::new(),
            when_to_use: String::new(),
            tools: vec!["tasks.list".into()],
            required_caps: vec![],
            body: String::new(),
            path: std::path::PathBuf::from("skills/tasks/SKILL.md"),
        };
        let installed = HashSet::from(["tasks".to_string()]);
        let merged = merge_skill_tools_for_modules(&[], &[skill], &installed);
        assert!(merged.iter().any(|t| t == "tasks.list"));
    }

    #[test]
    fn module_fallback_allowed_only_for_discovered_prefix() {
        let tools = discover_module_tools_from_list(&[sample_tasks_module()], |_| {
            Some(sample_tasks_describe())
        });
        assert!(module_fallback_allowed("tasks.create", &tools));
        assert!(!module_fallback_allowed("notes.create", &tools));
    }

    #[test]
    fn discovers_create_tools_from_module_list() {
        use aos_proto::create_contract::{INVOKE_CAP, TOOL_IDS};
        use aos_proto::ModuleInfo;

        let module = ModuleInfo {
            name: "create".into(),
            version: "1.0.0".into(),
            granted_caps: vec![INVOKE_CAP.into()],
            tools: TOOL_IDS.iter().map(|s| s.to_string()).collect(),
            quarantined: false,
            ui_mode: Some("declarative_ui".into()),
            ui_title: Some("Create".into()),
        };
        let tools = discover_module_tools_from_list(&[module], |_| None);
        for id in TOOL_IDS {
            let tool = tools
                .iter()
                .find(|t| t.name == *id)
                .unwrap_or_else(|| panic!("missing {id}"));
            assert_eq!(tool.required_caps, vec![INVOKE_CAP.to_string()]);
        }
        let msg = tool_unavailable_message("create.history.list", "absent du catalogue modules actif");
        assert!(!msg.contains("create.history.list"), "{msg}");
    }
}
