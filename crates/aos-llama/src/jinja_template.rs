//! Jinja renderer for tool-aware GGUF prompts and templates that the native
//! heuristic matcher cannot recognize.
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
    tools: &[Json],
) -> Result<String, hf_chat_template::Error> {
    let tmpl = ChatTemplate::from_str(template_src)?;
    // Agent runtimes append trusted context as later system messages. Qwen's
    // embedded template accepts system content only before the first user turn.
    // Preserve system-message order and every non-system turn while presenting
    // the instruction layer as a single initial message.
    let systems: Vec<_> = messages.iter().filter(|(role, _)| role == "system").map(|(_, content)| content.as_str()).collect();
    let mut msgs: Vec<Message> = Vec::with_capacity(messages.len());
    if !systems.is_empty() {
        msgs.push(Message::new("system", systems.join("\n\n")));
    }
    msgs.extend(messages
        .iter()
        .filter(|(role, _)| role != "system")
        .map(|(role, content)| Message::new(role.clone(), content.clone()))
    );
    let mut extra = Map::new();
    extra.insert("enable_thinking".into(), Json::Bool(false));
    let input = RenderInput {
        messages: msgs,
        tools: tools.to_vec(),
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

    #[test]
    fn later_system_context_is_preserved_before_user_turns() {
        let template = "{% for message in messages %}{% if message.role == 'system' and not loop.first %}{{ raise_exception('Unexpected message role') }}{% endif %}{{ message.role }}:{{ message.content }}|{% endfor %}";
        let messages = vec![("system".into(), "Initial rules".into()), ("user".into(), "Question".into()), ("assistant".into(), "Earlier answer".into()), ("system".into(), "Project context".into()), ("user".into(), "Follow-up".into())];
        let rendered = apply_jinja_chat_template(template, &messages, true, &[]).unwrap();
        assert_eq!(rendered, "system:Initial rules\n\nProject context|user:Question|assistant:Earlier answer|user:Follow-up|");
    }

    #[test]
    fn native_chatml_matcher_cannot_replace_tool_aware_rendering() {
        use llama_cpp_sys_2 as sys;
        use std::ffi::CString;

        let template = "{% if tools %}<|im_start|>system\n{{ tools|tojson }}<|im_end|>\n{% endif %}{% for message in messages %}<|im_start|>{{ message.role }}\n{{ message.content }}<|im_end|>\n{% endfor %}{% if add_generation_prompt %}<|im_start|>assistant\n{% endif %}";
        let ctemplate = CString::new(template).unwrap();
        let role = CString::new("user").unwrap();
        let content = CString::new("List my notes.").unwrap();
        let raw = sys::llama_chat_message {
            role: role.as_ptr(),
            content: content.as_ptr(),
        };
        let mut buffer = [0_i8; 8192];
        let n = unsafe {
            sys::llama_chat_apply_template(
                ctemplate.as_ptr(),
                &raw,
                1,
                true,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            )
        };
        assert!(
            n > 0 && (n as usize) <= buffer.len(),
            "native matcher recognizes ChatML"
        );
        let native: Vec<u8> = buffer[..n as usize].iter().map(|b| *b as u8).collect();
        assert!(!String::from_utf8(native).unwrap().contains("notes.list"));

        let messages = vec![("user".into(), "List my notes.".into())];
        let tools = vec![serde_json::json!({"type":"function","function":{"name":"notes.list","parameters":{"type":"object","properties":{}}}})];
        let rendered = apply_jinja_chat_template(template, &messages, true, &tools).unwrap();
        assert!(rendered.contains("notes.list"));
        assert!(rendered.contains("parameters"));
    }

    fn gemma4_fixture() -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/gemma4-e4b-chat-template.jinja");
        std::fs::read_to_string(path).expect("gemma4 fixture")
    }

    #[test]
    fn gemma4_text_only_user_message() {
        let template = gemma4_fixture();
        let messages = vec![("user".to_string(), "Hello".to_string())];
        let rendered = apply_jinja_chat_template(&template, &messages, true, &[])
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
        let rendered = apply_jinja_chat_template(&template, &messages, true, &[])
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
        let rendered = apply_jinja_chat_template(&template, &messages, true, &[])
            .expect("multi-turn gemma4");
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
        let out = apply_jinja_chat_template(&template, &messages, true, &[])
            .expect("jinja fallback");
        assert!(out.contains("ping"));
    }

    #[test]
    fn gemma4_renders_native_tool_declarations() {
        let template = gemma4_fixture();
        let messages = vec![("user".to_string(), "dessine un chat".to_string())];
        let tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "canvas.get",
                "description": "Lire la scène Canvas",
                "parameters": {"type": "object", "properties": {}}
            }
        })];
        let out = apply_jinja_chat_template(&template, &messages, true, &tools)
            .expect("gemma4 native tool declaration");
        assert!(out.contains("<|tool>"));
        assert!(out.contains("declaration:canvas.get"));
        assert!(out.contains("<tool|>"));
    }
}
