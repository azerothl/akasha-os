//! Compilation du prompt système agentic (couches empilées).

use aos_proto::{AgentGoal, AgentSpec, DocumentRef, PREVIEW_SURFACE_BRIEF, SYSTEM_ASSISTANT_PROMPT};

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

    // 1. Base OS + brief produit Preview
    parts.push(SYSTEM_ASSISTANT_PROMPT.to_string());
    parts.push(PREVIEW_SURFACE_BRIEF.to_string());

    // 2. Identité
    let mut identity = format!(
        "Tu es l'agent `{}` d'Akasha OS.",
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
        let mut skill_block = String::from(
            "## Skills actives (recettes — ce ne sont PAS des outils)\n\
             N'utilise JAMAIS le nom d'une skill comme champ `action`.\n\
             Choisis un outil listé sous chaque skill (ou dans le catalogue).\n",
        );
        for s in input.skills {
            let tools = if s.tools.is_empty() {
                "(voir corps)".to_string()
            } else {
                s.tools
                    .iter()
                    .map(|t| format!("`{t}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            skill_block.push_str(&format!(
                "### {}\n{}\nOutils autorisés pour cette skill : {tools}\n\n{}\n\n",
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

IMPORTANT :
- Commence directement par `{` ou ```json — PAS de balises `<think>`, `</think>`, ni monologue « Thinking Process ».
- Mets le raisonnement court UNIQUEMENT dans le champ JSON `thought` (1–2 phrases).
- `action` doit être EXACTEMENT un nom d'outil du catalogue (ex. `web.search`, `notes.create`, `fs.write`).
- INTERDIT d'utiliser un nom de skill comme action (`research`, `file-author`, `notes-writer`, `planner`, etc.).
- Les skills sont des recettes : lis leurs outils autorisés et appelle ces outils un par un.
- Le runtime a déjà classé la tâche (`task.assess` → simple|complex).
- Si complex : appelle `plan.update` AVANT tout effet de bord (recherche, écriture, spawn).
- Si le plan a des nœuds **indépendants**, préfère `agent.spawn` en parallèle puis `agent.await` plutôt que tout faire en série toi-même.
- `agent.spawn` : brief **court** (≤ 3 phrases, auto-suffisant), tools/docs **minimaux** — ne dump pas le contexte parent ni les résultats d'outils.
- Notes / documents longs : `notes.create` avec titre + **outline court**, puis `notes.update` **section par section** (≤ ~1200 caractères de `content` par appel). Ne mets jamais un guide entier dans un seul JSON.
- `memory.recall` sert à accélérer le nœud / brief courant — pas à relire tout le goal.
- Après une découverte utile : `memory.remember`.
- Avant une recherche web ou un fetch : `memory.recall` sur la requête courante si le contexte mémoire n'est pas déjà suffisant.
- Pour lire une page HTML utilise `web.browse` (texte). `net.fetch` ne fait que télécharger un fichier.

Actions runtime :
- plan.update : {"nodes":[{"id":"1","title":"...","status":"Pending"}]}
- agent.spawn : {"brief":"tâche étroite auto-suffisante (court)","skills":[],"tools":[],"documents":[]}
- agent.await : {"child_id":"..."}  (uniquement un id renvoyé par ton agent.spawn ; si spawn a échoué, continue toi-même)
- user.ask : {"question":"...","choices":["option A","option B"]}
  Si une info utilisateur manque (format, préférence, décision), pose UNE question et attends. N'invente pas.
  Sans réponse (timeout ~10 min), tu reçois un résultat d'outil et tu continues — ne repose pas la même question.
- memory.remember : {"text":"..."}
- memory.recall : {"query":"..."}
- docs.read : {"path":"..."}
- goal.complete : {"summary":"..."}
- goal.fail : {"reason":"..."}

Extensions OS (si limitation) :
- cap.request : {"cap":"tool.invoke:foo","reason":"..."}
- skill.create : {"name":"...","description":"...","body":"...","tools":[]}
- skill.activate : {"name":"..."}
- module.scaffold : {"name":"...","kind":"script|rust","description":"...","ui":"..."}  (ui optionnel : JSON declarative_ui)
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
        "Réécris un prompt système concis et efficace pour un agent Akasha OS.\n\
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
            kind: Default::default(),
            display_name: None,
            persona_id: None,
            system_prompt: Some("Sois bref.".into()),
            skills: vec![],
            tools: vec!["notes.create".into()],
            mcp_servers: vec![],
            documents: vec![],
            caps: vec!["tool.invoke:notes".into()],
            model_id: None,
            parent_id: None,
            session_id: None,
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
        assert!(!out.contains("Mémoire d'abord"));
        assert!(out.contains("task.assess"));
        assert!(out.contains("plan.update"));
    }
}
