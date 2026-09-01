//! Bus commands (UI → runtime) and events (runtime → UI).

use aos_agent::schedule::ScheduleEntry;
use aos_proto::{
    AgentInfo, AgentTrace, AuditEvent, CapInfo, ChatAttachment, ChatRoomMember, ChatSessionMeta,
    ChatSessionMode, CanvasOp, CanvasOpBody, CanvasPenStyle, DocumentRef, FeedbackSubmitRequest,
    FeedbackSubmitResponse, MemHit, ModelInfo, ModuleCatalogue, ModuleInfo, PendingConfirmation,
    ProviderRecord, SkillInfo, SkillPassPendingOffer, SystemMetrics, WebSearchHit,
};
use aos_proto::decl_ui::ModuleUiResponse;
use aos_proto::{McpServerInfo};
use crate::notes_panel;
use crate::tasks_panel;

#[allow(clippy::large_enum_variant)] // Boxing command payloads would complicate every UI dispatch site.
pub(crate) enum Cmd {
    /// Chat session active : historique + texte user (persisté côté platformd).
    Chat {
        session_id: String,
        history: Vec<(String, String)>,
        user_text: String,
        model_id: Option<String>,
        /// Chemins locaux PNG/JPEG pour le tour user (vision / mtmd).
        images: Vec<String>,
        /// Documents locaux (PDF/txt/md) — texte extrait côté runtime, pas vision.
        documents: Vec<DocumentRef>,
        /// E14 : déclencher mem.extract après le tour (Settings, défaut ON).
        auto_remember: bool,
        max_steps: u32,
        routing: String,
        /// Prefs UI language (`fr` / `en`) for product-doc injection.
        language: String,
        /// Session canvas panel open — enables draw/revise agent delegation.
        canvas_open: bool,
        canvas_aspect: aos_proto::CanvasAspect,
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
    MemSweepStatus,
    MemDelete { id: u64 },
    MemWipeUser,
    MemSupersede { id: u64, text: String },
    MemEdit { id: u64, text: String },
    SkillPassPending,
    SkillPassCreate { pattern_id: String },
    SkillPassDismiss { pattern_id: String },
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
    UserLibraryList,
    UserLibraryAdd { path: String },
    UserLibraryRemove { id: String },
    Confirm { id: String, approved: bool },
    AgentCreate {
        display_name: String,
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
        /// `slash` | `assistant` | `form` | `library`
        origin: String,
        /// When true and `session_id` is a Room session, add agent to salon roster after create.
        join_active_room: bool,
        /// Agents-tab library entry: persist roster spec only, never spawn a worker.
        library: bool,
    },
    AgentKill { id: String },
    AgentPause { id: String },
    AgentResume { id: String },
    AgentRetry { id: String },
    AgentSteer { id: String, text: String },
    AgentActDecision {
        agent_id: String,
        act_id: String,
        approved: bool,
    },
    AgentTrace { id: String },
    AgentExport { id: String },
    #[allow(dead_code)] // Runtime support is kept for the prompt-optimizer UI integration.
    AgentPromptOptimize {
        goal: String,
        skills: Vec<String>,
        tools: Vec<String>,
        current: Option<String>,
    },
    AgentCatalogRefresh,
    AgentSpecGet {
        id: String,
    },
    AgentRosterUpdate {
        agent_id: String,
        display_name: String,
        role: String,
        system_prompt: Option<String>,
        skills: Vec<String>,
        tools: Vec<String>,
        mcp_servers: Vec<String>,
        model_id: Option<String>,
    },
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
        next_fire_ms: Option<u64>,
        display_title: Option<String>,
    },
    ScheduleCancel {
        id: String,
    },
    SchedulePause {
        id: String,
    },
    ScheduleResume {
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
    ModelDownload { model_id: String },
    ModelDownloadHf {
        url: String,
        name: Option<String>,
    },
    ModelRemove { model_id: String },
    ModelRedownload { model_id: String },
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
        /// Logical `/downloads/...` path when not using the default image dest.
        output_path: Option<String>,
        enrich_prompt: bool,
        /// Prose rewrite via chat LLM before generation.
        enhance_prompt_chat: bool,
        /// When set, sent to sd.cpp as-is (skip LLM enrichment).
        generation_prompt: Option<String>,
        /// Visual composition blocks (normalized rects); merged into JSON/text after enrich.
        composition_blocks: Vec<crate::image_composition::CompositionBlock>,
    },
    /// Upscale an existing image (sd.cpp `--mode upscale`).
    MediaImageUpscale {
        source_path: String,
        upscale_model: String,
        upscale_repeats: u32,
        upscale_tile_size: u32,
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
    SessionSetMode {
        session_id: String,
        mode: ChatSessionMode,
    },
    SessionMembersAdd {
        session_id: String,
        member: ChatRoomMember,
    },
    SessionMembersRemove {
        session_id: String,
        agent_id: String,
    },
    RoomAddPersona {
        session_id: String,
        persona_id: String,
        model_id: Option<String>,
    },
    RoomTurn {
        session_id: String,
        content: String,
        images: Vec<String>,
    },
    RoomTurnCancel {
        session_id: String,
    },
    CanvasSetOpen {
        session_id: String,
        open: bool,
    },
    CanvasSetAspect {
        session_id: String,
        aspect: aos_proto::CanvasAspect,
    },
    CanvasApply {
        session_id: String,
        author_id: String,
        op: CanvasOpBody,
    },
    CanvasSetStyle {
        session_id: String,
        color: Option<String>,
        width: Option<f32>,
    },
    CanvasPoll {
        session_id: String,
        after_seq: Option<u64>,
    },
    CanvasExport {
        session_id: String,
        aspect: aos_proto::CanvasAspect,
        format: String,
    },
    CanvasEdit {
        session_id: String,
        author_id: String,
        edit: aos_proto::CanvasEdit,
    },
    /// Append chat sans infer (slash /agent, etc.).
    SessionAppend {
        session_id: String,
        role: String,
        content: String,
        attachments: Vec<ChatAttachment>,
    },
    ChatCancel {
        inference_id: u64,
        session_id: String,
    },
    CatalogueRefresh,
    CatalogueSetSource {
        enabled: bool,
    },
    CatalogueFetchExtra,
    CatalogueInstall {
        name: String,
    },
    SkillUninstall {
        name: String,
    },
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
    /// Spawn a document-prep agent (research + file-author) from a choice card.
    DocumentPrepSpawn {
        session_id: String,
        question: String,
        language: String,
        model_id: Option<String>,
        max_steps: u32,
    },
}

pub(crate) enum Evt {
    Delta {
        session_id: String,
        text: String,
    },
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
    AgentSpecLoaded {
        spec: aos_proto::AgentSpec,
    },
    AgentRosterSaved,
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
    UserLibraryListed(Vec<aos_proto::UserLibraryDoc>),
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
    ScheduleCreated(ScheduleEntry),
    ScheduleUpdated(ScheduleEntry),
    TasksListed(Vec<tasks_panel::TaskItem>),
    Confirms(Vec<PendingConfirmation>),
    FeedbackOk(FeedbackSubmitResponse),
    /// Préremplit le formulaire Retour (dépannage) sans publier tout de suite.
    FeedbackDraft(FeedbackSubmitRequest),
    Sessions(Vec<ChatSessionMeta>),
    /// Runtime names the session about to be loaded (create/delete/bootstrap).
    SessionLoadIntent { id: String },
    SessionLoaded {
        id: String,
        messages: Vec<ChatLine>,
        meta: ChatSessionMeta,
    },
    RoomTurnDone {
        session_id: String,
        agent_turns: u32,
        cancelled: bool,
    },
    CanvasMeta(ChatSessionMeta),
    CanvasSnapshot {
        session_id: String,
        canvas_open: bool,
        next_seq: u64,
        ops: Vec<CanvasOp>,
        pen: CanvasPenStyle,
        /// True when this is a delta poll (merge); false = full replace.
        delta: bool,
        /// When set, updates live-vision outline state from `canvas.get` poll.
        canvas_seeing: Option<bool>,
        layers: Vec<aos_proto::CanvasLayer>,
        active_layer_id: String,
    },
    CanvasExported {
        path: String,
        session_id: String,
    },
    MemHits(Vec<MemHit>),
    MemExtracted { n: usize },
    MemSweepStatus {
        last_pass_ms: u64,
        last_pass_label: String,
    },
    SkillPassPending(Option<SkillPassPendingOffer>),
    SkillPassCreated {
        pattern_id: String,
    },
    SecretList {
        names: Vec<String>,
        encrypted: bool,
    },
    WebResults(Vec<WebSearchHit>),
    BrowsePreview(String),
    NetMode(bool),
    SessionExported {
        path: String,
        session_id: String,
    },
    AgentExported {
        path: String,
        agent_id: String,
    },
    FileOk(String),
    MediaOk {
        kind: String,
        path: String,
        bytes: u64,
        engine: String,
        prompt: String,
        generation_prompt: Option<String>,
        /// Studio composition at generate time (image only).
        composition_blocks: Vec<crate::image_composition::CompositionBlock>,
        #[allow(dead_code)]
        model_id: String,
    },
    MediaImageEnriched {
        enriched: String,
    },
    MediaImageStarted {
        enriching: bool,
        upscaling: bool,
        total_steps: u32,
    },
    MediaImageProgress {
        enriching: bool,
        upscaling: bool,
        step: u32,
        total_steps: u32,
        elapsed_secs: u64,
    },
    Skills(Vec<SkillInfo>),
    McpServers(Vec<McpServerInfo>),
    PromptOptimized(String),
    Models(Vec<ModelInfo>),
    ModelDownloadStarted { model_id: String },
    ModelDownloadProgress {
        model_id: String,
        done_bytes: u64,
        total_bytes: u64,
        percent: u8,
    },
    ModelDownloadFinished { model_id: String },
    ModelDownloadFailed {
        model_id: String,
        error: String,
    },
    Providers(Vec<ProviderRecord>),
    ProviderTested {
        ok: bool,
        message: String,
        models: Vec<String>,
    },
    AgentTrace(AgentTrace),
    InferStarted {
        session_id: String,
        inference_id: u64,
    },
    ChatCancelled { session_id: String },
    Catalogue(ModuleCatalogue),
    InstalledSkills(Vec<aos_proto::SkillInfo>),
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
    pub(crate) speaker_id: Option<String>,
    pub(crate) speaker_name: Option<String>,
    pub(crate) thinking: Option<String>,
}

impl ChatLine {
    pub(crate) fn plain(role: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            text: text.into(),
            attachments: Vec::new(),
            speaker_id: None,
            speaker_name: None,
            thinking: None,
        }
    }

}

#[derive(Debug, Clone)]
pub(crate) struct AgentNotice {
    pub(crate) agent_id: String,
    pub(crate) session_id: String,
    pub(crate) summary: String,
}
