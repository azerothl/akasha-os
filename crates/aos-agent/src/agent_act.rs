//! Human French phrases for agent tool actions (tester-cohort slice 1).

use serde_json::Value;

/// Gate mode for chat-delegated agents (`ask` prompts before each mutating act).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentGateMode {
    Ask,
    Autonomous,
}

impl AgentGateMode {
    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("autonomous") || s.eq_ignore_ascii_case("auto") {
            Self::Autonomous
        } else {
            Self::Ask
        }
    }

    pub fn as_pref_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Autonomous => "autonomous",
        }
    }
}

/// Whether this tool action should pause for inline Allow Once / Deny in chat.
pub fn requires_act_gate(action: &str) -> bool {
    let name = action.trim();
    if name.is_empty()
        || matches!(
            name,
            "noop"
                | "goal.complete"
                | "goal.fail"
                | "task.assess"
                | "plan.update"
                | "user.ask"
                | "agent.spawn"
                | "agent.await"
                | "agent.create"
                | "cap.request"
        )
    {
        return false;
    }
    if name.starts_with("notes.") {
        return !matches!(name, "notes.list" | "notes.read" | "notes.search" | "notes.related" | "notes.links");
    }
    if name.starts_with("tasks.") {
        return !matches!(name, "tasks.list");
    }
    if name.starts_with("canvas.") {
        return !matches!(name, "canvas.get" | "canvas.export");
    }
    if name.starts_with("fs.") {
        return matches!(name, "fs.write" | "fs.delete" | "fs.mkdir");
    }
    if name.starts_with("media.") {
        return true;
    }
    if name.starts_with("module.") || name == "skill.create" {
        return false;
    }
    if name.starts_with("web.") || name == "net.fetch" {
        return true;
    }
    if name == "mem.episodic_write" {
        return true;
    }
    false
}

fn arg_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max).collect::<String>())
}

/// French human sentence for a pending agent act (no snake_case, no JSON).
pub fn phrase_fr(action: &str, args: &Value) -> String {
    let name = action.trim();
    match name {
        "notes.create" => {
            if let Some(title) = arg_str(args, "title") {
                format!("L'agent veut créer une note intitulée « {title} ».")
            } else {
                "L'agent veut créer une note.".into()
            }
        }
        "notes.update" => {
            if let Some(title) = arg_str(args, "title").or_else(|| arg_str(args, "slug")) {
                format!("L'agent veut modifier la note « {title} ».")
            } else {
                "L'agent veut modifier une note.".into()
            }
        }
        "tasks.create" => {
            if let Some(title) = arg_str(args, "title") {
                format!("L'agent veut créer la tâche « {title} ».")
            } else {
                "L'agent veut créer une tâche.".into()
            }
        }
        "tasks.update" | "tasks.complete" => {
            if let Some(id) = arg_str(args, "id").or_else(|| arg_str(args, "task_id")) {
                format!("L'agent veut mettre à jour la tâche {id}.")
            } else {
                "L'agent veut mettre à jour une tâche.".into()
            }
        }
        "canvas.set_style" => "L'agent veut changer le style du crayon sur le canvas.".into(),
        "canvas.stroke" | "canvas.line" | "canvas.spline" => {
            "L'agent veut tracer sur le canvas.".into()
        }
        "canvas.rect" => "L'agent veut dessiner un rectangle sur le canvas.".into(),
        "canvas.ellipse" => "L'agent veut dessiner une ellipse sur le canvas.".into(),
        "canvas.erase" => "L'agent veut effacer une zone du canvas.".into(),
        "canvas.clear" => "L'agent veut effacer le canvas.".into(),
        "canvas.undo" => "L'agent veut annuler le dernier trait sur le canvas.".into(),
        "fs.write" => {
            if let Some(path) = arg_str(args, "path") {
                format!("L'agent veut écrire le fichier {path}.")
            } else {
                "L'agent veut écrire un fichier.".into()
            }
        }
        "fs.delete" => {
            if let Some(path) = arg_str(args, "path") {
                format!("L'agent veut supprimer le fichier {path}.")
            } else {
                "L'agent veut supprimer un fichier.".into()
            }
        }
        "fs.mkdir" => {
            if let Some(path) = arg_str(args, "path") {
                format!("L'agent veut créer le dossier {path}.")
            } else {
                "L'agent veut créer un dossier.".into()
            }
        }
        "media.image.generate" | "media.generate" => {
            if let Some(prompt) = arg_str(args, "prompt") {
                format!(
                    "L'agent veut générer une image : « {} ».",
                    truncate(&prompt, 80)
                )
            } else {
                "L'agent veut générer une image.".into()
            }
        }
        "media.audio.generate" => "L'agent veut générer un fichier audio.".into(),
        "web.search" => {
            if let Some(q) = arg_str(args, "query") {
                format!("L'agent veut rechercher sur le web : « {} ».", truncate(&q, 80))
            } else {
                "L'agent veut faire une recherche web.".into()
            }
        }
        "web.browse" => {
            if let Some(url) = arg_str(args, "url") {
                format!("L'agent veut parcourir la page {url}.")
            } else {
                "L'agent veut parcourir une page web.".into()
            }
        }
        "net.fetch" => {
            if let Some(url) = arg_str(args, "url") {
                format!("L'agent veut télécharger {url}.")
            } else {
                "L'agent veut télécharger une URL.".into()
            }
        }
        "mem.episodic_write" => "L'agent veut enregistrer un souvenir.".into(),
        other if other.contains('.') => {
            let verb = other.split('.').last().unwrap_or("agir");
            let surface = other.split('.').next().unwrap_or("l'outil");
            match surface {
                "notes" => format!("L'agent veut agir sur les notes ({verb})."),
                "tasks" => format!("L'agent veut agir sur les tâches ({verb})."),
                "canvas" => format!("L'agent veut modifier le canvas ({verb})."),
                _ => format!("L'agent veut effectuer une action ({verb})."),
            }
        }
        _ => "L'agent veut effectuer une action.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn gate_mutating_notes_not_list() {
        assert!(requires_act_gate("notes.create"));
        assert!(!requires_act_gate("notes.list"));
    }

    #[test]
    fn gate_canvas_mutate_not_get() {
        assert!(requires_act_gate("canvas.stroke"));
        assert!(!requires_act_gate("canvas.get"));
    }

    #[test]
    fn phrase_notes_create_with_title() {
        let p = phrase_fr("notes.create", &json!({"title": "cohort", "body": "hello"}));
        assert!(p.contains("cohort"));
        assert!(!p.contains("notes.create"));
        assert!(!p.contains('{'));
    }

    #[test]
    fn phrase_canvas_stroke() {
        let p = phrase_fr("canvas.stroke", &json!({}));
        assert!(p.contains("tracer"));
        assert!(!p.contains("canvas.stroke"));
    }

    #[test]
    fn gate_mode_parse() {
        assert_eq!(AgentGateMode::parse("ask"), AgentGateMode::Ask);
        assert_eq!(AgentGateMode::parse("autonomous"), AgentGateMode::Autonomous);
    }
}
