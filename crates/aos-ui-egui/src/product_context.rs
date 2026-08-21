//! Micro product brief for the chat system prompt (detail via RAG).

use aos_proto::PREVIEW_SURFACE_BRIEF;

/// Build the small product-context block (no FEATURES stuffing — RAG is in mem.context).
pub fn chat_product_context(version: &str, _language: &str) -> String {
    format!("## Contexte produit (Preview {version})\n{PREVIEW_SURFACE_BRIEF}\n")
}
