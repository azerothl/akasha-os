//! Localized human sentences for inline agent-act gates in the chat thread.

use aos_proto::ChatAttachment;
use serde_json::Value;

use crate::i18n::UiStrings;

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

/// Format a pending agent act as a human sentence (EN or FR).
pub fn format_agent_act_phrase(lang: &str, action: &str, args: &Value) -> String {
    let en = lang.eq_ignore_ascii_case("en");
    let name = action.trim();
    match name {
        "notes.create" => {
            if let Some(title) = arg_str(args, "title") {
                if en {
                    format!("The agent wants to create a note titled « {title} ».")
                } else {
                    format!("L'agent veut créer une note intitulée « {title} ».")
                }
            } else if en {
                "The agent wants to create a note.".into()
            } else {
                "L'agent veut créer une note.".into()
            }
        }
        "notes.update" => {
            if let Some(title) = arg_str(args, "title").or_else(|| arg_str(args, "slug")) {
                if en {
                    format!("The agent wants to edit the note « {title} ».")
                } else {
                    format!("L'agent veut modifier la note « {title} ».")
                }
            } else if en {
                "The agent wants to edit a note.".into()
            } else {
                "L'agent veut modifier une note.".into()
            }
        }
        "tasks.create" => {
            if let Some(title) = arg_str(args, "title") {
                if en {
                    format!("The agent wants to create the task « {title} ».")
                } else {
                    format!("L'agent veut créer la tâche « {title} ».")
                }
            } else if en {
                "The agent wants to create a task.".into()
            } else {
                "L'agent veut créer une tâche.".into()
            }
        }
        "tasks.update" | "tasks.complete" => {
            if let Some(id) = arg_str(args, "id").or_else(|| arg_str(args, "task_id")) {
                if en {
                    format!("The agent wants to update task {id}.")
                } else {
                    format!("L'agent veut mettre à jour la tâche {id}.")
                }
            } else if en {
                "The agent wants to update a task.".into()
            } else {
                "L'agent veut mettre à jour une tâche.".into()
            }
        }
        "canvas.set_style" => {
            if en {
                "The agent wants to change the pen style on the canvas.".into()
            } else {
                "L'agent veut changer le style du crayon sur le canvas.".into()
            }
        }
        "canvas.stroke" | "canvas.line" | "canvas.spline" => {
            if en {
                "The agent wants to draw on the canvas.".into()
            } else {
                "L'agent veut tracer sur le canvas.".into()
            }
        }
        "canvas.rect" => {
            if en {
                "The agent wants to draw a rectangle on the canvas.".into()
            } else {
                "L'agent veut dessiner un rectangle sur le canvas.".into()
            }
        }
        "canvas.ellipse" => {
            if en {
                "The agent wants to draw an ellipse on the canvas.".into()
            } else {
                "L'agent veut dessiner une ellipse sur le canvas.".into()
            }
        }
        "canvas.erase" => {
            if en {
                "The agent wants to erase an area on the canvas.".into()
            } else {
                "L'agent veut effacer une zone du canvas.".into()
            }
        }
        "canvas.clear" => {
            if en {
                "The agent wants to clear the canvas.".into()
            } else {
                "L'agent veut effacer le canvas.".into()
            }
        }
        "canvas.undo" => {
            if en {
                "The agent wants to undo the last stroke on the canvas.".into()
            } else {
                "L'agent veut annuler le dernier trait sur le canvas.".into()
            }
        }
        "fs.write" => {
            if let Some(path) = arg_str(args, "path") {
                if en {
                    format!("The agent wants to write the file {path}.")
                } else {
                    format!("L'agent veut écrire le fichier {path}.")
                }
            } else if en {
                "The agent wants to write a file.".into()
            } else {
                "L'agent veut écrire un fichier.".into()
            }
        }
        "fs.delete" => {
            if let Some(path) = arg_str(args, "path") {
                if en {
                    format!("The agent wants to delete the file {path}.")
                } else {
                    format!("L'agent veut supprimer le fichier {path}.")
                }
            } else if en {
                "The agent wants to delete a file.".into()
            } else {
                "L'agent veut supprimer un fichier.".into()
            }
        }
        "fs.mkdir" => {
            if let Some(path) = arg_str(args, "path") {
                if en {
                    format!("The agent wants to create the folder {path}.")
                } else {
                    format!("L'agent veut créer le dossier {path}.")
                }
            } else if en {
                "The agent wants to create a folder.".into()
            } else {
                "L'agent veut créer un dossier.".into()
            }
        }
        "media.image.generate" | "media.generate" => {
            if let Some(prompt) = arg_str(args, "prompt") {
                let p = truncate(&prompt, 80);
                if en {
                    format!("The agent wants to generate an image: « {p} ».")
                } else {
                    format!("L'agent veut générer une image : « {p} ».")
                }
            } else if en {
                "The agent wants to generate an image.".into()
            } else {
                "L'agent veut générer une image.".into()
            }
        }
        "media.audio.generate" => {
            if en {
                "The agent wants to generate an audio file.".into()
            } else {
                "L'agent veut générer un fichier audio.".into()
            }
        }
        "web.search" => {
            if let Some(q) = arg_str(args, "query") {
                let q = truncate(&q, 80);
                if en {
                    format!("The agent wants to search the web: « {q} ».")
                } else {
                    format!("L'agent veut rechercher sur le web : « {q} ».")
                }
            } else if en {
                "The agent wants to search the web.".into()
            } else {
                "L'agent veut faire une recherche web.".into()
            }
        }
        "web.browse" => {
            if let Some(url) = arg_str(args, "url") {
                if en {
                    format!("The agent wants to browse {url}.")
                } else {
                    format!("L'agent veut parcourir la page {url}.")
                }
            } else if en {
                "The agent wants to browse a web page.".into()
            } else {
                "L'agent veut parcourir une page web.".into()
            }
        }
        "net.fetch" => {
            if let Some(url) = arg_str(args, "url") {
                if en {
                    format!("The agent wants to download {url}.")
                } else {
                    format!("L'agent veut télécharger {url}.")
                }
            } else if en {
                "The agent wants to download a URL.".into()
            } else {
                "L'agent veut télécharger une URL.".into()
            }
        }
        "mem.episodic_write" => {
            if en {
                "The agent wants to save a memory.".into()
            } else {
                "L'agent veut enregistrer un souvenir.".into()
            }
        }
        other if other.contains('.') => {
            let verb = other.split('.').last().unwrap_or("agir");
            let surface = other.split('.').next().unwrap_or("l'outil");
            match surface {
                "notes" => {
                    if en {
                        format!("The agent wants to act on notes ({verb}).")
                    } else {
                        format!("L'agent veut agir sur les notes ({verb}).")
                    }
                }
                "tasks" => {
                    if en {
                        format!("The agent wants to act on tasks ({verb}).")
                    } else {
                        format!("L'agent veut agir sur les tâches ({verb}).")
                    }
                }
                "canvas" => {
                    if en {
                        format!("The agent wants to modify the canvas ({verb}).")
                    } else {
                        format!("L'agent veut modifier le canvas ({verb}).")
                    }
                }
                _ => {
                    if en {
                        format!("The agent wants to perform an action ({verb}).")
                    } else {
                        format!("L'agent veut effectuer une action ({verb}).")
                    }
                }
            }
        }
        _ => {
            if en {
                "The agent wants to perform an action.".into()
            } else {
                "L'agent veut effectuer une action.".into()
            }
        }
    }
}

/// Thread bubble text for an agent-act attachment (pending, approved, or denied).
pub fn thread_display_text(
    t: &UiStrings,
    lang: &str,
    action: &str,
    args: &Value,
    state: &str,
    legacy_phrase: &str,
) -> String {
    let detail = if action.is_empty() {
        legacy_phrase.to_string()
    } else {
        format_agent_act_phrase(lang, action, args)
    };
    match state {
        "approved" => format!("{} — {detail}", t.agent_act_resolved_approved),
        "denied" => format!("{} — {detail}", t.agent_act_resolved_denied),
        _ => detail,
    }
}

/// Thread bubble text from a chat attachment, if this message carries an agent act.
pub fn thread_display_from_attachment(
    t: &UiStrings,
    lang: &str,
    att: &ChatAttachment,
) -> Option<String> {
    match att {
        ChatAttachment::AgentAct {
            action,
            args,
            state,
            phrase,
            ..
        } if !action.is_empty() || !phrase.is_empty() => Some(thread_display_text(
            t,
            lang,
            action,
            args,
            state,
            phrase,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn en_fr_differ_for_same_action() {
        let args = json!({"title": "cohort"});
        let en = format_agent_act_phrase("en", "notes.create", &args);
        let fr = format_agent_act_phrase("fr", "notes.create", &args);
        assert_ne!(en, fr);
        assert!(en.contains("cohort"));
        assert!(fr.contains("cohort"));
        assert!(en.contains("create"));
        assert!(fr.contains("créer"));
    }

    #[test]
    fn canvas_stroke_avoids_raw_action_id() {
        let en = format_agent_act_phrase("en", "canvas.stroke", &json!({}));
        assert!(!en.contains("canvas.stroke"));
        assert!(en.contains("draw"));
    }

    #[test]
    fn resolved_approved_uses_i18n_prefix() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        let args = json!({});
        let en = thread_display_text(&t_en, "en", "canvas.stroke", &args, "approved", "");
        let fr = thread_display_text(&t_fr, "fr", "canvas.stroke", &args, "approved", "");
        assert!(en.starts_with("Allowed once"));
        assert!(fr.starts_with("Autorisé une fois"));
        assert_ne!(en, fr);
    }
}
