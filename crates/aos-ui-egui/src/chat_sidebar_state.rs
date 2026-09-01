//! Mutable state for the conversation sidebar tools.

use aos_proto::WebSearchHit;

#[derive(Debug)]
pub(crate) struct ChatSidebarState {
    pub(crate) rename: String,
    pub(crate) web_query: String,
    pub(crate) web_results: Vec<WebSearchHit>,
    pub(crate) fetch_url: String,
    pub(crate) browse_preview: String,
    pub(crate) generated_format: String,
    pub(crate) generated_content: String,
    pub(crate) generated_path: String,
}

impl Default for ChatSidebarState {
    fn default() -> Self {
        Self {
            rename: String::new(),
            web_query: String::new(),
            web_results: Vec::new(),
            fetch_url: String::new(),
            browse_preview: String::new(),
            generated_format: "md".into(),
            generated_content: String::new(),
            generated_path: "/downloads/note.md".into(),
        }
    }
}
