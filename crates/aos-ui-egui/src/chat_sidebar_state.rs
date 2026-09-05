//! Mutable state for the conversation sidebar tools.

use aos_proto::WebSearchHit;

#[derive(Debug)]
pub(crate) struct ChatSidebarState {
    pub(crate) search: String,
    pub(crate) show_archived: bool,
    pub(crate) delete_confirm: Option<String>,
    pub(crate) rename: String,
    /// Popup de renommage ouverte (plus de champ inline 120px qui débordait).
    #[allow(dead_code)]
    pub(crate) rename_open: bool,
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
            search: String::new(),
            show_archived: false,
            delete_confirm: None,
            rename: String::new(),
            rename_open: false,
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
