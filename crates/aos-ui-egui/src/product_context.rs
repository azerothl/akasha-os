//! Micro product brief for the chat system prompt (detail via RAG).

use aos_proto::format_preview_surface_brief;

/// Build the small product-context block (no FEATURES stuffing — RAG is in mem.context).
pub fn chat_product_context(version: &str, _language: &str) -> String {
    format!("{}\n", format_preview_surface_brief(version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_product_context_uses_single_version() {
        let version = "1.2.3";
        let block = chat_product_context(version, "en");
        assert_eq!(block.matches(version).count(), 1);
        assert!(!block.to_lowercase().contains("bootable"));
    }
}
