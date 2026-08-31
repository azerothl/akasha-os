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

fn subst(template: &str, key: &str, value: &str) -> String {
    template.replace(&format!("{{{key}}}"), value)
}

/// Format a pending agent act as a human sentence from UI i18n keys.
pub fn format_agent_act_phrase(t: &UiStrings, action: &str, args: &Value) -> String {
    let name = action.trim();
    match name {
        "notes.create" => {
            if let Some(title) = arg_str(args, "title") {
                subst(t.agent_act_notes_create_title, "title", &title)
            } else {
                t.agent_act_notes_create.into()
            }
        }
        "notes.update" => {
            if let Some(title) = arg_str(args, "title").or_else(|| arg_str(args, "slug")) {
                subst(t.agent_act_notes_update_title, "title", &title)
            } else {
                t.agent_act_notes_update.into()
            }
        }
        "tasks.create" => {
            if let Some(title) = arg_str(args, "title") {
                subst(t.agent_act_tasks_create_title, "title", &title)
            } else {
                t.agent_act_tasks_create.into()
            }
        }
        "tasks.update" | "tasks.complete" => {
            if let Some(id) = arg_str(args, "id").or_else(|| arg_str(args, "task_id")) {
                subst(t.agent_act_tasks_update_id, "id", &id)
            } else {
                t.agent_act_tasks_update.into()
            }
        }
        "canvas.set_style" => t.agent_act_canvas_set_style.into(),
        "canvas.stroke" | "canvas.line" | "canvas.spline" | "canvas.path" => t.agent_act_canvas_stroke.into(),
        "canvas.rect" => t.agent_act_canvas_rect.into(),
        "canvas.ellipse" => t.agent_act_canvas_ellipse.into(),
        "canvas.erase" => t.agent_act_canvas_erase.into(),
        "canvas.clear" => t.agent_act_canvas_clear.into(),
        "canvas.undo" => t.agent_act_canvas_undo.into(),
        "fs.write" => {
            if let Some(path) = arg_str(args, "path") {
                subst(t.agent_act_fs_write_path, "path", &path)
            } else {
                t.agent_act_fs_write.into()
            }
        }
        "fs.delete" => {
            if let Some(path) = arg_str(args, "path") {
                subst(t.agent_act_fs_delete_path, "path", &path)
            } else {
                t.agent_act_fs_delete.into()
            }
        }
        "fs.mkdir" => {
            if let Some(path) = arg_str(args, "path") {
                subst(t.agent_act_fs_mkdir_path, "path", &path)
            } else {
                t.agent_act_fs_mkdir.into()
            }
        }
        "media.image.generate" | "media.generate" => {
            if let Some(prompt) = arg_str(args, "prompt") {
                subst(
                    t.agent_act_media_image_prompt,
                    "prompt",
                    &truncate(&prompt, 80),
                )
            } else {
                t.agent_act_media_image.into()
            }
        }
        "media.audio.generate" => t.agent_act_media_audio.into(),
        "web.search" => {
            if let Some(q) = arg_str(args, "query") {
                subst(t.agent_act_web_search_query, "query", &truncate(&q, 80))
            } else {
                t.agent_act_web_search.into()
            }
        }
        "web.browse" => {
            if let Some(url) = arg_str(args, "url") {
                subst(t.agent_act_web_browse_url, "url", &url)
            } else {
                t.agent_act_web_browse.into()
            }
        }
        "net.fetch" => {
            if let Some(url) = arg_str(args, "url") {
                subst(t.agent_act_net_fetch_url, "url", &url)
            } else {
                t.agent_act_net_fetch.into()
            }
        }
        "mem.episodic_write" => t.agent_act_mem_episodic_write.into(),
        _ => t.agent_act_generic.into(),
    }
}

/// Thread bubble text for an agent-act attachment (pending, approved, or denied).
pub fn thread_display_text(
    t: &UiStrings,
    action: &str,
    args: &Value,
    state: &str,
    legacy_phrase: &str,
) -> String {
    let detail = if action.is_empty() {
        legacy_phrase.to_string()
    } else {
        format_agent_act_phrase(t, action, args)
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
    att: &ChatAttachment,
) -> Option<String> {
    match att {
        ChatAttachment::AgentAct {
            action,
            args,
            state,
            phrase,
            ..
        } if !action.is_empty() || !phrase.is_empty() => {
            Some(thread_display_text(t, action, args, state, phrase))
        }
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
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        let en = format_agent_act_phrase(&t_en, "notes.create", &args);
        let fr = format_agent_act_phrase(&t_fr, "notes.create", &args);
        assert_ne!(en, fr);
        assert!(en.contains("cohort"));
        assert!(fr.contains("cohort"));
        assert!(en.contains("create"));
        assert!(fr.contains("créer"));
    }

    #[test]
    fn canvas_stroke_avoids_raw_action_id() {
        let t = crate::i18n::strings("en");
        let en = format_agent_act_phrase(&t, "canvas.stroke", &json!({}));
        assert!(!en.contains("canvas.stroke"));
        assert!(en.contains("draw"));
    }

    #[test]
    fn resolved_approved_uses_i18n_prefix() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        let args = json!({});
        let en = thread_display_text(&t_en, "canvas.stroke", &args, "approved", "");
        let fr = thread_display_text(&t_fr, "canvas.stroke", &args, "approved", "");
        assert!(en.starts_with("Allowed once"));
        assert!(fr.starts_with("Autorisé une fois"));
        assert_ne!(en, fr);
    }

    #[test]
    fn unknown_action_uses_generic_without_tool_leak() {
        let t_en = crate::i18n::strings("en");
        let t_fr = crate::i18n::strings("fr");
        for action in ["notes.archive", "canvas.rotate", "module.invoke", "noop"] {
            let en = format_agent_act_phrase(&t_en, action, &json!({}));
            let fr = format_agent_act_phrase(&t_fr, action, &json!({}));
            assert_eq!(en, t_en.agent_act_generic);
            assert_eq!(fr, t_fr.agent_act_generic);
            assert!(!en.contains(action));
            assert!(!fr.contains(action));
            assert!(!en.contains("archive"));
            assert!(!en.contains("rotate"));
            assert!(!en.contains("invoke"));
        }
    }
}
