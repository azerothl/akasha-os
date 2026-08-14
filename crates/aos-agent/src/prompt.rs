//! Compilation du prompt système agentic (couches empilées).

use aos_proto::{AgentGoal, AgentSpec, DocumentRef, SYSTEM_ASSISTANT_PROMPT};

use crate::skills::SkillDoc;
use crate::tools::ToolDesc;

/// Contexte pour compiler le prompt.
pub struct PromptCompileInput<'a> {
    pub spec: &'a AgentSpec,
    pub skills: &'a [SkillDoc],
    pub tools: &'a [ToolDesc],
    pub doc_index: &'a [DocumentRef],
}

/// Compile le prompt système multi-couches.
pub fn compile_system_prompt(input: &PromptCompileInput<'_>) -> String {
    let mut parts: Vec<String> = Vec::new();

    // 1. Base OS
    parts.push(SYSTEM_ASSISTANT_PROMPT.to_string());

    // 2. Identité
    let mut identity = format!(
        "Tu es l'agent `{}` d'Agent OS.",
        input.spec.agent_id
    );
    if let Some(parent) = &input.spec.parent_id {
        identity.push_str(&format!(" Tu es un sous-agent de `{parent}`."));
    }
    if !input.spec.caps.is_empty() {
        identity.push_str(&format!(
            " Caps actives : {}.",
            input.spec.caps.join(", ")
        ));
    }
    parts.push(identity);

    // 3. Prompt utilisateur
    if let Some(user_prompt) = &input.spec.system_prompt {
        if !user_prompt.trim().is_empty() {
            parts.push(format!("## Instructions utilisateur\n{}", user_prompt.trim()));
        }
    }

    // 4. Goal
    parts.push(format_goal(&input.spec.goal));

    // 5. Skills
    if !input.skills.is_empty() {
        let mut skill_block = String::from("## Skills actives\n");
        for s in input.skills {
            skill_block.push_str(&format!(
                "### {}\n{}\n\n{}\n\n",
                s.name, s.description, s.body
            ));
        }
        parts.push(skill_block);
    }

    // 6. Catalogue outils
    if !input.tools.is_empty() {
        let mut tools_block = String::from("## Catalogue d'outils (utilise uniquement ceux-ci)\n");
        for t in input.tools {
            tools_block.push_str(&format!(
                "- `{}` : {} | schema: {}\n",
                t.name,
                t.description,
                t.input_schema
            ));
        }
        parts.push(tools_block);
    }

    // 7. Index documents
    if !input.doc_index.is_empty() {
        let mut docs = String::from("## Documents attachés\n");
        for d in input.doc_index {
            let label = if d.label.is_empty() {
                d.path.as_str()
            } else {
                d.label.as_str()
            };
            docs.push_str(&format!("- `{label}` → path `{}` (utiliser `docs.read`)\n", d.path));
        }
        parts.push(docs);
    }

    // 8. Protocole d'actions
    parts.push(ACTION_PROTOCOL.to_string());

    parts.join("\n\n")
}

fn format_goal(goal: &AgentGoal) -> String {
    let mut s = format!(
        "## Goal\n{}\n\nContraintes : max_steps={}, max_subagents={}, timeout_secs={}.",
        goal.statement, goal.max_steps, goal.max_subagents, goal.timeout_secs
    );
    if !goal.success_criteria.is_empty() {
        s.push_str("\nCritères de succès :\n");
        for c in &goal.success_criteria {
            s.push_str(&format!("- {c}\n"));
        }
    }
    s
}

const ACTION_PROTOCOL: &str = r#"## Protocole d'actions

Réponds par un objet JSON unique (éventuellement dans un bloc ```json) de la forme :
{"thought":"raisonnement court","action":"<nom>","args":{...}}

Actions runtime :
- plan.update : {"nodes":[{"id":"1","title":"...","status":"Pending"}]}
- agent.spawn : {"brief":"...","skills":[],"tools":[],"documents":[]}
- agent.await : {"child_id":"..."}
- memory.remember : {"text":"..."}
- memory.recall : {"query":"..."}
- docs.read : {"path":"..."}
- goal.complete : {"summary":"..."}
- goal.fail : {"reason":"..."}

Extensions OS (si limitation) :
- cap.request : {"cap":"tool.invoke:foo","reason":"..."}
- skill.create : {"name":"...","description":"...","body":"...","tools":[]}
- skill.activate : {"name":"..."}
- module.scaffold : {"name":"...","kind":"script|rust","description":"..."}
- module.package : {"name":"..."}  (script/ext-rt, sans rustc)
- module.compile : {"name":"..."}  (Rust→WASM, confirmation)
- module.install : {"source_dir":"..."}  (après package/compile)

Ou appelle un outil du catalogue avec action = nom de l'outil.
Compat : une ligne `TOOL: <outil> <args json>` est aussi acceptée.

Ne termine qu'avec goal.complete quand les critères sont remplis."#;

/// Prompt court pour optimiser le system prompt.
pub fn optimize_prompt_request(
    goal: &str,
    skills: &[String],
    tools: &[String],
    current: Option<&str>,
) -> String {
    format!(
        "Réécris un prompt système concis et efficace pour un agent Agent OS.\n\
         Goal : {goal}\n\
         Skills : {}\n\
         Outils : {}\n\
         Prompt actuel (optionnel) : {}\n\
         Réponds UNIQUEMENT avec le nouveau prompt système, sans préambule.",
        skills.join(", "),
        tools.join(", "),
        current.unwrap_or("(aucun)")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{AgentBudget, AgentGoal};

    #[test]
    fn compiles_layers() {
        let spec = AgentSpec {
            agent_id: "agent-1".into(),
            goal: AgentGoal {
                statement: "écrire une note".into(),
                ..Default::default()
            },
            system_prompt: Some("Sois bref.".into()),
            skills: vec![],
            tools: vec!["notes.create".into()],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec!["tool.invoke:notes".into()],
            model_id: None,
            parent_id: None,
            budget: AgentBudget::default(),
            optimize_prompt: false,
        };
        let tools = vec![ToolDesc {
            name: "notes.create".into(),
            description: "crée une note".into(),
            input_schema: serde_json::json!({"type":"object"}),
            backend: crate::tools::ToolBackend::Module,
            required_caps: vec!["tool.invoke:notes".into()],
        }];
        let out = compile_system_prompt(&PromptCompileInput {
            spec: &spec,
            skills: &[],
            tools: &tools,
            doc_index: &[],
        });
        assert!(out.contains("agent-1"));
        assert!(out.contains("écrire une note"));
        assert!(out.contains("notes.create"));
        assert!(out.contains("Sois bref"));
    }
}
