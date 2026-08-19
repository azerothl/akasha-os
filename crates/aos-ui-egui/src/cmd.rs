//! Bus commands (UI → runtime) and events (runtime → UI).

use aos_agent::schedule::ScheduleEntry;
use aos_proto::{
    AgentInfo, AgentTrace, AuditEvent, CapInfo, ChatAttachment, ChatSessionMeta, DocumentRef,
    FeedbackSubmitRequest, FeedbackSubmitResponse, MemHit, ModelInfo, ModuleCatalogue,
    ModuleInfo, PendingConfirmation, ProviderRecord, SkillInfo, SystemMetrics, WebSearchHit,
};
use aos_proto::decl_ui::ModuleUiResponse;
use aos_proto::{McpServerInfo};
use crate::notes_panel;
use crate::tasks_panel;

pub(crate) enum Cmd {
    /// Chat session active : historique + texte user (persisté côté platformd).
    Chat {
        session_id: String,
        history: Vec<(String, String)>,
        user_text: String,
        model_id: Option<String>,
        /// E14 : déclencher mem.extract après le tour (Settings, défaut ON).
        auto_remember: bool,
        max_steps: u32,
        routing: String,
    },
    SessionBootstrap,
    SessionCreate { title: Option<String> },
    SessionSelect { id: String },
    SessionRename { id: String, title: String },
    SessionDelete { id: String },
    SessionExport { id: String },
    MemRecall { query: String },
    MemRemember { text: String, pinned: bool },
    MemList { include_superseded: bool },
    MemDelete { id: u64 },
    MemWipeUser,
    MemSupersede { id: u64, text: String },
    MemEdit { id: u64, text: String },
    SecretSet { name: String, value: String },
    SecretList,
    NetSetMode { online: bool },
    SetRouting { mode: String },
    WebSearch { query: String, engine: String },
    WebBrowse { url: String, max_chars: usize },
    NetFetch { url: String, max_bytes: u64 },
    FilesGenerate {
        format: String,
        path: String,
        content: String,
        title: Option<String>,
    },
    Help,
    NotesList,
    NotesCreate { title: String, content: String },
    NotesUpdate {
        title: String,
        path: String,
        content: String,
    },
    NotesRead {
        title: Option<String>,
        path: Option<String>,
        slug: Option<String>,
    },
    NotesSearch { query: String },
    NotesRelated {
        path: String,
        topic: String,
    },
    Confirm { id: String, approved: bool },
    AgentCreate {
        task: String,
        system_prompt: Option<String>,
        skills: Vec<String>,
        tools: Vec<String>,
        mcp_servers: Vec<String>,
        documents: Vec<DocumentRef>,
        optimize_prompt: bool,
        max_steps: u32,
        timeout_secs: u64,
        model_id: Option<String>,
        session_id: Option<String>,
        /// `slash` | `assistant` | `form`
        origin: String,
    },
    AgentKill { id: String },
    AgentPause { id: String },
    AgentResume { id: String },
    AgentRetry { id: String },
    AgentSteer { id: String, text: String },
    AgentTrace { id: String },
    AgentPromptOptimize {
        goal: String,
        skills: Vec<String>,
        tools: Vec<String>,
        current: Option<String>,
    },
    AgentCatalogRefresh,
    Troubleshoot,
    Audit { last: usize },
    CapList { holder: String },
    CapRevoke {
        holder: String,
        cap_id: u64,
        tree: bool,
    },
    ScheduleList,
    ScheduleCreate {
        goal: String,
        interval_secs: u64,
    },
    ScheduleCancel {
        id: String,
    },
    TasksList,
    TasksCreate {
        title: String,
        notes: String,
    },
    TasksComplete {
        id: String,
        done: bool,
    },
    Feedback(FeedbackSubmitRequest),
    KillAuditd,
    #[allow(dead_code)]
    RestartModeld,
    MigrateModeld { target: String },
    RefreshConfirms,
    ModelsRefresh,
    ModelLoad { model_id: String },
    ProviderList,
    ProviderUpsert {
        provider: ProviderRecord,
        secret_value: Option<String>,
    },
    ProviderRemove { id: String },
    ProviderTest { id: String },
    MediaImage {
        prompt: String,
        model_id: Option<String>,
        options: aos_proto::MediaImageOptions,
    },
    MediaAudio {
        text: String,
        model_id: Option<String>,
        options: aos_proto::MediaAudioOptions,
    },
    SessionSetModel {
        session_id: String,
        model_id: Option<String>,
    },
    /// Append chat sans infer (slash /agent, etc.).
    SessionAppend {
        session_id: String,
        role: String,
        content: String,
        attachments: Vec<ChatAttachment>,
    },
    ChatCancel { inference_id: u64 },
    CatalogueRefresh,
    ModuleList,
    ModuleInstall {
        source_dir: String,
        approved_caps: Option<Vec<String>>,
    },
    ModuleUninstall {
        name: String,
    },
    ModuleUiLoad {
        module: String,
    },
    ModuleUiRefresh {
        module: String,
    },
    ModuleUiBind {
        module: String,
        tool: String,
    },
    ModuleUiInvoke {
        module: String,
        tool: String,
        args: serde_json::Value,
    },
}

pub(crate) enum Evt {
    Delta(String),
    Done {
        text: String,
        session_id: String,
        attachments: Vec<ChatAttachment>,
    },
    Error(String),
    Status(String),
    ChatSystem(String),
    Metrics(SystemMetrics),
    Agents(Vec<AgentInfo>),
    AgentSpawned {
        session_id: String,
        agent_id: String,
        title: String,
        origin: String,
        ack: String,
    },
    NotesListed(Vec<notes_panel::NoteListItem>),
    NoteLoaded(notes_panel::NoteDetail),
    NotesSearchHits(Vec<notes_panel::NoteSearchHit>),
    NotesRelated(Vec<notes_panel::NoteRelatedHit>),
    NotesSaved {
        path: String,
        slug: String,
        title: String,
    },
    /// Payload brut (compat scénarios / debug).
    Notes(String),
    Audit(Vec<AuditEvent>),
    Caps {
        holder: String,
        caps: Vec<CapInfo>,
    },
    Schedules(Vec<ScheduleEntry>),
    TasksListed(Vec<tasks_panel::TaskItem>),
    Confirms(Vec<PendingConfirmation>),
    FeedbackOk(FeedbackSubmitResponse),
    /// Préremplit le formulaire Retour (dépannage) sans publier tout de suite.
    FeedbackDraft(FeedbackSubmitRequest),
    Sessions(Vec<ChatSessionMeta>),
    SessionLoaded {
        id: String,
        messages: Vec<ChatLine>,
    },
    MemHits(Vec<MemHit>),
    MemExtracted { n: usize },
    SecretList {
        names: Vec<String>,
        encrypted: bool,
    },
    WebResults(Vec<WebSearchHit>),
    BrowsePreview(String),
    NetMode(bool),
    FileOk(String),
    MediaOk {
        kind: String,
        path: String,
        bytes: u64,
        engine: String,
        prompt: String,
    },
    Skills(Vec<SkillInfo>),
    McpServers(Vec<McpServerInfo>),
    PromptOptimized(String),
    Models(Vec<ModelInfo>),
    Providers(Vec<ProviderRecord>),
    ProviderTested {
        ok: bool,
        message: String,
        models: Vec<String>,
    },
    AgentTrace(AgentTrace),
    InferStarted { inference_id: u64 },
    ChatCancelled,
    Catalogue(ModuleCatalogue),
    InstalledModules(Vec<ModuleInfo>),
    ModuleInstalled(String),
    ModuleUninstalled(String),
    ModuleUiLoaded(ModuleUiResponse),
    ModuleUiFailed {
        module: String,
        error: String,
    },
    ModuleUiBind {
        module: String,
        tool: String,
        result: serde_json::Value,
        error: Option<String>,
    },
    ModuleUiInvokeDone {
        module: String,
        tool: String,
        ok: bool,
        result: serde_json::Value,
        error: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ChatLine {
    pub(crate) role: String,
    pub(crate) text: String,
    pub(crate) attachments: Vec<ChatAttachment>,
}

impl ChatLine {
    pub(crate) fn plain(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            text: text.into(),
            attachments: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AgentNotice {
    pub(crate) agent_id: String,
    pub(crate) session_id: String,
    pub(crate) summary: String,
}
