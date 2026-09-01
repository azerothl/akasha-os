//! Fallback Jinja renderer for GGUF chat templates that `llama_chat_apply_template`
//! cannot match (heuristic substring detection only — no real Jinja parser).
//!
//! Gemma 4 ships a large tool-calling Jinja template in GGUF metadata; native
//! apply returns `-1` (UNKNOWN). This module renders the embedded template with
//! `hf-chat-template` (minijinja + transformers pycompat), matching the context
//! llama.cpp's `common_chat_template_direct_apply` supplies for plain chat.

use hf_chat_template::{ChatTemplate, Message, RenderInput};
use serde_json::{Map, Value as Json};

/// Render `template_src` with `messages` (`role`, `content` pairs).
///
/// `add_generation_prompt` mirrors the `add_ass` flag passed to
/// `llama_chat_apply_template`.
pub fn apply_jinja_chat_template(
    template_src: &str,
    messages: &[(String, String)],
    add_generation_prompt: bool,
) -> Result<String, hf_chat_template::Error> {
    let tmpl = ChatTemplate::from_str(template_src)?;
    let msgs: Vec<Message> = messages
        .iter()
        .map(|(role, content)| Message::new(role.clone(), content.clone()))
        .collect();
    let mut extra = Map::new();
    extra.insert("enable_thinking".into(), Json::Bool(false));
    let input = RenderInput {
        messages: msgs,
        add_generation_prompt,
        extra,
        ..Default::default()
    };
    tmpl.render(&input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn gemma4_fixture() -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/gemma4-e4b-chat-template.jinja");
        std::fs::read_to_string(path).expect("gemma4 fixture")
    }

    #[test]
    fn gemma4_text_only_user_message() {
        let template = gemma4_fixture();
        let messages = vec![("user".to_string(), "Hello".to_string())];
        let rendered = apply_jinja_chat_template(&template, &messages, true)
            .expect("gemma4 template should render");
        assert!(rendered.contains("<|turn>user"));
        assert!(rendered.contains("Hello"));
        assert!(rendered.contains("<|turn>model"));
    }

    #[test]
    fn gemma4_user_with_media_marker() {
        let template = gemma4_fixture();
        let messages = vec![(
            "user".to_string(),
            "<__media__>\nDescribe this image".to_string(),
        )];
        let rendered = apply_jinja_chat_template(&template, &messages, true)
            .expect("gemma4 vision marker in user content");
        assert!(rendered.contains("<__media__>"));
        assert!(rendered.contains("Describe this image"));
        assert!(rendered.contains("<|turn>model"));
    }

    #[test]
    fn gemma4_multi_turn_assistant_role() {
        let template = gemma4_fixture();
        let messages = vec![
            ("user".to_string(), "Hi".to_string()),
            ("assistant".to_string(), "Hello!".to_string()),
            ("user".to_string(), "Again".to_string()),
        ];
        let rendered =
            apply_jinja_chat_template(&template, &messages, true).expect("multi-turn gemma4");
        assert!(rendered.contains("<|turn>user"));
        assert!(rendered.contains("<|turn>model"));
        assert!(rendered.contains("Again"));
    }

    #[test]
    fn native_apply_would_fail_on_gemma4_fixture() {
        // Document regression: llama_chat_apply_template heuristic matcher returns -1
        // for the Gemma 4 canonical template (no <start_of_turn>, complex Jinja macros).
        let template = gemma4_fixture();
        assert!(
            !template.contains("<start_of_turn>"),
            "fixture should be Gemma 4, not legacy Gemma"
        );
        assert!(
            template.contains("'<|tool_call>call:'"),
            "fixture should contain Gemma 4 tool-call signature"
        );
        // Rendering must succeed via jinja fallback.
        let messages = vec![("user".to_string(), "ping".to_string())];
        let out = apply_jinja_chat_template(&template, &messages, true).expect("jinja fallback");
        assert!(out.contains("ping"));
    }
}
