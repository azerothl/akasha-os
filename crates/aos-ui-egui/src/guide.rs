//! In-app feature guides (EN/FR) — scrollable help surface, not a website or mill.

use crate::Tab;
use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuideTopic {
    #[default]
    Overview,
    Create,
    Chat,
    Canvas,
    Agents,
    Memory,
    Salon,
}

impl GuideTopic {
    pub const ALL: [GuideTopic; 7] = [
        GuideTopic::Overview,
        GuideTopic::Create,
        GuideTopic::Chat,
        GuideTopic::Canvas,
        GuideTopic::Agents,
        GuideTopic::Memory,
        GuideTopic::Salon,
    ];
}

#[derive(Debug, Clone, Default)]
pub struct GuideState {
    pub open: bool,
    pub topic: GuideTopic,
}

impl GuideState {
    pub fn open_topic(&mut self, topic: GuideTopic) {
        self.topic = topic;
        self.open = true;
    }
}

pub fn topic_for_tab(tab: &Tab) -> Option<GuideTopic> {
    match tab {
        Tab::Chat => Some(GuideTopic::Chat),
        Tab::Image => Some(GuideTopic::Create),
        Tab::Agents => Some(GuideTopic::Agents),
        Tab::Memory => Some(GuideTopic::Memory),
        _ => None,
    }
}

/// Small help affordance for tab headings (opens guide at `topic`).
pub fn tab_help_button(ui: &mut egui::Ui, tooltip: &str) -> bool {
    ui.small_button("?")
        .on_hover_text(tooltip)
        .clicked()
}

pub struct GuideStrings {
    pub title: &'static str,
    pub close: &'static str,
    pub help_tooltip: &'static str,
    pub restart_onboarding: &'static str,
    pub nav_overview: &'static str,
    pub nav_create: &'static str,
    pub nav_chat: &'static str,
    pub nav_canvas: &'static str,
    pub nav_agents: &'static str,
    pub nav_memory: &'static str,
    pub nav_salon: &'static str,
    pub overview_intro: &'static str,
    pub overview_tour_chat: &'static str,
    pub overview_tour_create: &'static str,
    pub overview_tour_memory: &'static str,
    pub overview_tour_agents: &'static str,
    pub overview_tour_canvas: &'static str,
    pub overview_tour_salon: &'static str,
    pub section_what: &'static str,
    pub section_impact: &'static str,
    pub section_example: &'static str,
    pub create_intro: &'static str,
    pub create_prompt_title: &'static str,
    pub create_prompt_body: &'static str,
    pub create_prompt_example: &'static str,
    pub create_reference_title: &'static str,
    pub create_reference_body: &'static str,
    pub create_reference_example: &'static str,
    pub create_inpaint_title: &'static str,
    pub create_inpaint_body: &'static str,
    pub create_inpaint_example: &'static str,
    pub create_video_title: &'static str,
    pub create_video_body: &'static str,
    pub create_video_example: &'static str,
    pub create_format_title: &'static str,
    pub create_format_body: &'static str,
    pub create_format_example: &'static str,
    pub create_generate_title: &'static str,
    pub create_generate_body: &'static str,
    pub create_generate_example: &'static str,
    pub chat_intro: &'static str,
    pub chat_opt_sessions_title: &'static str,
    pub chat_opt_sessions_body: &'static str,
    pub chat_opt_attach_title: &'static str,
    pub chat_opt_attach_body: &'static str,
    pub chat_opt_slash_title: &'static str,
    pub chat_opt_slash_body: &'static str,
    pub canvas_intro: &'static str,
    pub canvas_opt_toggle_title: &'static str,
    pub canvas_opt_toggle_body: &'static str,
    pub canvas_opt_tools_title: &'static str,
    pub canvas_opt_tools_body: &'static str,
    pub canvas_opt_export_title: &'static str,
    pub canvas_opt_export_body: &'static str,
    pub agents_intro: &'static str,
    pub agents_opt_goal_title: &'static str,
    pub agents_opt_goal_body: &'static str,
    pub agents_opt_tools_title: &'static str,
    pub agents_opt_tools_body: &'static str,
    pub agents_opt_history_title: &'static str,
    pub agents_opt_history_body: &'static str,
    pub memory_intro: &'static str,
    pub memory_opt_remember_title: &'static str,
    pub memory_opt_remember_body: &'static str,
    pub memory_opt_recall_title: &'static str,
    pub memory_opt_recall_body: &'static str,
    pub memory_opt_list_title: &'static str,
    pub memory_opt_list_body: &'static str,
    pub salon_intro: &'static str,
    pub salon_opt_enable_title: &'static str,
    pub salon_opt_enable_body: &'static str,
    pub salon_opt_members_title: &'static str,
    pub salon_opt_members_body: &'static str,
    pub salon_opt_turn_title: &'static str,
    pub salon_opt_turn_body: &'static str,
}

const EN: GuideStrings = GuideStrings {
    title: "Guide — Akasha OS Preview",
    close: "Close",
    help_tooltip: "Open the in-app guide for this area",
    restart_onboarding: "Re-run first-run tour",
    nav_overview: "Overview",
    nav_create: "Create",
    nav_chat: "Chat",
    nav_canvas: "Canvas",
    nav_agents: "Agents",
    nav_memory: "Memory",
    nav_salon: "Salon",
    overview_intro: "Short map of Preview surfaces. Pick a topic on the left for step-by-step help with examples.",
    overview_tour_chat: "Chat — parallel persisted conversations with the local model.",
    overview_tour_create: "Create — diffusion image and short video studio (prompt, reference, inpaint).",
    overview_tour_memory: "Memory — durable facts injected before each reply (remember / recall).",
    overview_tour_agents: "Agents — autonomous goal loops with tools, skills, and capabilities.",
    overview_tour_canvas: "Canvas — shared vector drawing on the active chat session (not Create pixels).",
    overview_tour_salon: "Salon — multi-agent room inside a chat session (built-in personas + library agents).",
    section_what: "What it does",
    section_impact: "What changes when you use it",
    section_example: "Example",
    create_intro: "Create turns a description into a still, or a short clip. You can start from a photo you already have, or paint over part of a result.",
    create_prompt_title: "Prompt",
    create_prompt_body: "Write what should appear: subject, place, light. Generate uses this text. Specific beats vague.",
    create_prompt_example: "Ex. \"A teal mug on a wooden desk, morning light\" → one object, warm light.",
    create_reference_title: "Reference photo",
    create_reference_body: "Paperclip next to the prompt. Create starts from that picture. Closer to the photo keeps pose and colors; looser follows the prompt.",
    create_reference_example: "Ex. Portrait + \"wearing a red scarf\" → same person, new scarf.",
    create_inpaint_title: "Fix a region",
    create_inpaint_body: "After a result, paint the part to change, then Generate. Only that area is redrawn.",
    create_inpaint_example: "Ex. Paint the sky + \"sunset clouds\" → new sky, same ground.",
    create_video_title: "Video",
    create_video_body: "Switch Image / Video at the top. Pick a length in seconds. Describe motion. The clip lands in Downloads.",
    create_video_example: "Ex. Video + \"steam rising from the mug\" → a short clip of motion.",
    create_format_title: "Size and quality",
    create_format_body: "Width and height set how large the still is. Fast / Balanced / Quality is how long it spends working: Quality is slower and usually sharper.",
    create_format_example: "Ex. 512×512 Fast for a draft; larger + Quality for a final still.",
    create_generate_title: "Generate",
    create_generate_body: "Runs Create with the current prompt and options. You need an image or video model installed. The result appears on the right; earlier results stay in history.",
    create_generate_example: "",
    chat_intro: "Default home: one conversation per session, offline-first. Messages persist; slash commands reach other surfaces without leaving chat.",
    chat_opt_sessions_title: "Sessions sidebar",
    chat_opt_sessions_body: "Create or switch sessions. Each keeps its own history, model choice, and salon/canvas toggles.",
    chat_opt_attach_title: "Attachments",
    chat_opt_attach_body: "Paperclip: images for vision models, documents for grounded Q&A. Pending chips show what ships with the next message.",
    chat_opt_slash_title: "Slash commands",
    chat_opt_slash_body: "Type / for completions — /help, /notes, /memory, and more route to platform tools.",
    canvas_intro: "Vector drawing layer on the active chat session — pen, shapes, eraser. Not the Create diffusion tab.",
    canvas_opt_toggle_title: "Canvas toggle",
    canvas_opt_toggle_body: "Enable Canvas in the session bar. Agents may draw only while Canvas is open; humans can always paint.",
    canvas_opt_tools_title: "Tools & layers",
    canvas_opt_tools_body: "Toolbar: pen, eraser, line, rect, ellipse, fill. Undo clears the last stroke; clear wipes the board (with confirm).",
    canvas_opt_export_title: "Export",
    canvas_opt_export_body: "Snapshot exports PNG under /downloads — a raster of the vector scene, not media.image.generate.",
    agents_intro: "Spawn background workers with a goal, model, and tool allow-list. Active runs show live traces; history keeps completed loops.",
    agents_opt_goal_title: "Goal & role",
    agents_opt_goal_body: "The task text and optional role steer planning. Max steps and timeout cap autonomy (Settings defaults apply).",
    agents_opt_tools_title: "Tools & capabilities",
    agents_opt_tools_body: "Pick skills, MCP servers, and caps. Sensitive actions still pass through the confirmation banner.",
    agents_opt_history_title: "History tab",
    agents_opt_history_body: "Review prior agent threads, open detail, or re-run with the same configuration.",
    memory_intro: "Long-term facts stored locally and injected before replies. Distinct from chat transcript — use for preferences and stable user context.",
    memory_opt_remember_title: "Remember",
    memory_opt_remember_body: "Save a fact explicitly (pinned by default). Auto-remember in Settings can extract facts after chat turns.",
    memory_opt_recall_title: "Recall",
    memory_opt_recall_body: "Semantic search over stored facts; hits show relevance and can be edited or superseded.",
    memory_opt_list_title: "List & sweep",
    memory_opt_list_body: "List all facts; toggle superseded. Daily sweep re-extracts and relates facts in the background.",
    salon_intro: "Room mode inside Chat: multiple personas reply in turn on the same session. Not external messaging — everything stays in-app.",
    salon_opt_enable_title: "Enable salon",
    salon_opt_enable_body: "Session bar → Enable room / Activer le salon. Switches from 1:1 assistant to multi-speaker transcript.",
    salon_opt_members_title: "Members",
    salon_opt_members_body: "Add built-in personas (Researcher, Critic, …) or agents from your library. Header strip shows roster; colors follow speaker id.",
    salon_opt_turn_title: "Room turn",
    salon_opt_turn_body: "Each user message queues a conductor turn; speakers answer in order. Cancel stops an in-flight turn.",
};

const FR: GuideStrings = GuideStrings {
    title: "Guide — Akasha OS Preview",
    close: "Fermer",
    help_tooltip: "Ouvrir le guide intégré pour cette zone",
    restart_onboarding: "Relancer le tour de premier lancement",
    nav_overview: "Vue d'ensemble",
    nav_create: "Créer",
    nav_chat: "Chat",
    nav_canvas: "Canvas",
    nav_agents: "Agents",
    nav_memory: "Mémoire",
    nav_salon: "Salon",
    overview_intro: "Carte rapide des surfaces Preview. Choisissez un sujet à gauche pour une aide pas à pas avec exemples.",
    overview_tour_chat: "Chat — conversations parallèles persistées avec le modèle local.",
    overview_tour_create: "Créer — studio diffusion images et courtes vidéos (prompt, référence, inpaint).",
    overview_tour_memory: "Mémoire — faits durables injectés avant chaque réponse (remember / recall).",
    overview_tour_agents: "Agents — boucles autonomes avec outils, skills et capacités.",
    overview_tour_canvas: "Canvas — dessin vectoriel partagé sur la session de chat active (pas les pixels Créer).",
    overview_tour_salon: "Salon — salle multi-agents dans une session de chat (personas intégrés + agents bibliothèque).",
    section_what: "À quoi ça sert",
    section_impact: "Ce que ça change",
    section_example: "Exemple",
    create_intro: "Create transforme une description en image, ou en court clip. Vous pouvez partir d'une photo déjà là, ou peindre une zone du résultat.",
    create_prompt_title: "Prompt",
    create_prompt_body: "Dites ce qui doit apparaître : sujet, lieu, lumière. Générer lit ce texte. Le précis bat le vague.",
    create_prompt_example: "Ex. « Une tasse bleue sur un bureau en bois, lumière du matin » → un objet, lumière chaude.",
    create_reference_title: "Image de référence",
    create_reference_body: "Trombone à côté du prompt. Create part de cette photo. Plus près de la photo : pose et couleurs. Plus loin : le prompt gagne.",
    create_reference_example: "Ex. Portrait + « avec une écharpe rouge » → même personne, nouvelle écharpe.",
    create_inpaint_title: "Corriger une zone",
    create_inpaint_body: "Après un résultat, peignez la partie à changer, puis Générer. Seule cette zone est redessinée.",
    create_inpaint_example: "Ex. Ciel + « nuages au coucher du soleil » → nouveau ciel, sol inchangé.",
    create_video_title: "Vidéo",
    create_video_body: "Bascule Image / Vidéo en haut. Choisissez une durée en secondes. Décrivez le mouvement. Le clip va dans Téléchargements.",
    create_video_example: "Ex. Vidéo + « vapeur qui monte de la tasse » → un court clip.",
    create_format_title: "Taille et qualité",
    create_format_body: "Largeur et hauteur = taille de l'image. Fast / Balanced / Quality = le temps passé : Quality plus lent, souvent plus net.",
    create_format_example: "Ex. 512×512 Fast pour un brouillon ; plus grand + Quality pour un final.",
    create_generate_title: "Générer",
    create_generate_body: "Lance Create avec le prompt et les options du moment. Il faut un modèle image ou vidéo installé. Le résultat s'affiche à droite ; l'historique garde les précédents.",
    create_generate_example: "",
    chat_intro: "Accueil par défaut : une conversation par session, hors ligne d'abord. Les messages persistent ; les commandes / ouvrent d'autres surfaces sans quitter le chat.",
    chat_opt_sessions_title: "Barre latérale Sessions",
    chat_opt_sessions_body: "Créez ou changez de session. Chacune garde son historique, modèle et bascules salon/canvas.",
    chat_opt_attach_title: "Pièces jointes",
    chat_opt_attach_body: "Trombone : images pour les modèles vision, documents pour des réponses ancrées. Les pastilles montrent ce qui part avec le prochain message.",
    chat_opt_slash_title: "Commandes /",
    chat_opt_slash_body: "Tapez / pour l'auto-complétion — /help, /notes, /memory, etc. routent vers les outils plateforme.",
    canvas_intro: "Couche de dessin vectoriel sur la session de chat active — stylo, formes, gomme. Pas l'onglet diffusion Créer.",
    canvas_opt_toggle_title: "Bascule Canvas",
    canvas_opt_toggle_body: "Activez Canvas dans la barre de session. Les agents dessinent seulement quand Canvas est ouvert ; l'humain peut toujours peindre.",
    canvas_opt_tools_title: "Outils",
    canvas_opt_tools_body: "Barre : stylo, gomme, ligne, rectangle, ellipse, remplissage. Annuler efface le dernier trait ; effacer vide le tableau (avec confirmation).",
    canvas_opt_export_title: "Export",
    canvas_opt_export_body: "L'instantané exporte un PNG sous /downloads — raster de la scène vectorielle, pas media.image.generate.",
    agents_intro: "Lancez des workers en arrière-plan avec un objectif, un modèle et une liste d'outils. Les runs actifs montrent le flux ; l'historique garde les boucles terminées.",
    agents_opt_goal_title: "Objectif et rôle",
    agents_opt_goal_body: "Le texte de tâche et le rôle optionnel orientent la planification. Étapes max et délai limitent l'autonomie (défauts dans Réglages).",
    agents_opt_tools_title: "Outils et capacités",
    agents_opt_tools_body: "Choisissez skills, serveurs MCP et caps. Les actions sensibles passent encore par la bannière de confirmation.",
    agents_opt_history_title: "Onglet Historique",
    agents_opt_history_body: "Consultez les fils passés, ouvrez le détail ou relancez avec la même configuration.",
    memory_intro: "Faits long terme stockés localement et injectés avant les réponses. Distinct du transcript — pour préférences et contexte stable.",
    memory_opt_remember_title: "Remember",
    memory_opt_remember_body: "Enregistre un fait explicitement (épinglé par défaut). L'auto-souvenir dans Réglages peut extraire des faits après les tours de chat.",
    memory_opt_recall_title: "Recall",
    memory_opt_recall_body: "Recherche sémantique dans les faits ; les résultats montrent la pertinence et peuvent être modifiés ou remplacés.",
    memory_opt_list_title: "Liste et balayage",
    memory_opt_list_body: "Liste tous les faits ; basculez les remplacés. Le balayage quotidien ré-extrait et relie les faits en arrière-plan.",
    salon_intro: "Mode salon dans Chat : plusieurs personas répondent à tour de rôle sur la même session. Pas de messagerie externe — tout reste dans l'app.",
    salon_opt_enable_title: "Activer le salon",
    salon_opt_enable_body: "Barre de session → Activer le salon / Enable room. Passe de l'assistant 1:1 au transcript multi-locuteurs.",
    salon_opt_members_title: "Membres",
    salon_opt_members_body: "Ajoutez des personas intégrés (Researcher, Critic, …) ou des agents de la bibliothèque. La bande d'en-tête montre le roster ; les couleurs suivent l'id locuteur.",
    salon_opt_turn_title: "Tour de salon",
    salon_opt_turn_body: "Chaque message utilisateur lance un tour conducteur ; les locuteurs répondent dans l'ordre. Annuler stoppe un tour en cours.",
};

pub fn strings(lang: &str) -> GuideStrings {
    if lang.eq_ignore_ascii_case("en") {
        EN
    } else {
        FR
    }
}

pub fn nav_label(g: &GuideStrings, topic: GuideTopic) -> &'static str {
    match topic {
        GuideTopic::Overview => g.nav_overview,
        GuideTopic::Create => g.nav_create,
        GuideTopic::Chat => g.nav_chat,
        GuideTopic::Canvas => g.nav_canvas,
        GuideTopic::Agents => g.nav_agents,
        GuideTopic::Memory => g.nav_memory,
        GuideTopic::Salon => g.nav_salon,
    }
}

pub fn show_window(
    ctx: &egui::Context,
    state: &mut GuideState,
    lang: &str,
    restart_onboarding: &mut bool,
) {
    if !state.open {
        return;
    }
    let g = strings(lang);
    let mut close = false;
    egui::Window::new(g.title)
        .collapsible(false)
        .resizable(true)
        .default_size([820.0, 560.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                let nav_w = 148.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(nav_w, ui.available_height().max(320.0)),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(nav_w);
                        for topic in GuideTopic::ALL {
                            let label = nav_label(&g, topic);
                            if ui
                                .selectable_label(state.topic == topic, label)
                                .clicked()
                            {
                                state.topic = topic;
                            }
                        }
                        ui.add_space(12.0);
                        ui.separator();
                        if ui.small_button(g.restart_onboarding).clicked() {
                            *restart_onboarding = true;
                            close = true;
                        }
                    },
                );
                ui.separator();
                let content_w = ui.available_width().max(200.0);
                let content_h = ui.available_height().max(280.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(content_w, content_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(content_w);
                        egui::ScrollArea::vertical()
                            .id_salt("guide_content")
                            .auto_shrink([false, false])
                            .max_height(content_h)
                            .show(ui, |ui| {
                                ui.set_min_width(content_w - 8.0);
                                render_topic(ui, state.topic, &g);
                            });
                    },
                );
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(g.close).clicked() {
                        close = true;
                    }
                });
            });
        });
    if close {
        state.open = false;
    }
}

fn render_topic(ui: &mut egui::Ui, topic: GuideTopic, g: &GuideStrings) {
    match topic {
        GuideTopic::Overview => render_overview(ui, g),
        GuideTopic::Create => render_create(ui, g),
        GuideTopic::Chat => render_stub(
            ui,
            g,
            g.chat_intro,
            &[
                (g.chat_opt_sessions_title, g.chat_opt_sessions_body),
                (g.chat_opt_attach_title, g.chat_opt_attach_body),
                (g.chat_opt_slash_title, g.chat_opt_slash_body),
            ],
            FigureKind::Chat,
        ),
        GuideTopic::Canvas => render_stub(
            ui,
            g,
            g.canvas_intro,
            &[
                (g.canvas_opt_toggle_title, g.canvas_opt_toggle_body),
                (g.canvas_opt_tools_title, g.canvas_opt_tools_body),
                (g.canvas_opt_export_title, g.canvas_opt_export_body),
            ],
            FigureKind::Canvas,
        ),
        GuideTopic::Agents => render_stub(
            ui,
            g,
            g.agents_intro,
            &[
                (g.agents_opt_goal_title, g.agents_opt_goal_body),
                (g.agents_opt_tools_title, g.agents_opt_tools_body),
                (g.agents_opt_history_title, g.agents_opt_history_body),
            ],
            FigureKind::Agents,
        ),
        GuideTopic::Memory => render_stub(
            ui,
            g,
            g.memory_intro,
            &[
                (g.memory_opt_remember_title, g.memory_opt_remember_body),
                (g.memory_opt_recall_title, g.memory_opt_recall_body),
                (g.memory_opt_list_title, g.memory_opt_list_body),
            ],
            FigureKind::Memory,
        ),
        GuideTopic::Salon => render_stub(
            ui,
            g,
            g.salon_intro,
            &[
                (g.salon_opt_enable_title, g.salon_opt_enable_body),
                (g.salon_opt_members_title, g.salon_opt_members_body),
                (g.salon_opt_turn_title, g.salon_opt_turn_body),
            ],
            FigureKind::Salon,
        ),
    }
}

fn render_overview(ui: &mut egui::Ui, g: &GuideStrings) {
    ui.heading(g.nav_overview);
    ui.label(g.overview_intro);
    ui.add_space(8.0);
    for line in [
        g.overview_tour_chat,
        g.overview_tour_create,
        g.overview_tour_canvas,
        g.overview_tour_salon,
        g.overview_tour_agents,
        g.overview_tour_memory,
    ] {
        ui.label(line);
    }
}

fn render_create(ui: &mut egui::Ui, g: &GuideStrings) {
    ui.heading(g.nav_create);
    ui.label(g.create_intro);
    ui.add_space(8.0);
    paint_figure(ui, FigureKind::CreateFlow);
    ui.add_space(8.0);
    guide_section(
        ui,
        g,
        g.create_prompt_title,
        g.create_prompt_body,
        None,
        g.create_prompt_example,
    );
    guide_section(
        ui,
        g,
        g.create_reference_title,
        g.create_reference_body,
        Some(FigureKind::Reference),
        g.create_reference_example,
    );
    guide_section(
        ui,
        g,
        g.create_inpaint_title,
        g.create_inpaint_body,
        Some(FigureKind::Inpaint),
        g.create_inpaint_example,
    );
    guide_section(
        ui,
        g,
        g.create_video_title,
        g.create_video_body,
        Some(FigureKind::Video),
        g.create_video_example,
    );
    guide_section(
        ui,
        g,
        g.create_format_title,
        g.create_format_body,
        Some(FigureKind::Format),
        g.create_format_example,
    );
    guide_section(
        ui,
        g,
        g.create_generate_title,
        g.create_generate_body,
        Some(FigureKind::Generate),
        g.create_generate_example,
    );
}

fn render_stub(
    ui: &mut egui::Ui,
    g: &GuideStrings,
    intro: &str,
    options: &[(&str, &str)],
    figure: FigureKind,
) {
    ui.label(intro);
    ui.add_space(6.0);
    paint_figure(ui, figure);
    ui.add_space(8.0);
    for (title, body) in options {
        ui.strong(*title);
        ui.label(*body);
        ui.add_space(6.0);
    }
}

fn guide_section(
    ui: &mut egui::Ui,
    _g: &GuideStrings,
    title: &str,
    body: &str,
    figure: Option<FigureKind>,
    example: &str,
) {
    ui.add_space(4.0);
    ui.strong(title);
    ui.weak(_g.section_what);
    ui.label(body);
    if let Some(kind) = figure {
        ui.add_space(4.0);
        paint_figure(ui, kind);
    }
    if !example.is_empty() {
        ui.weak(_g.section_example);
        ui.label(example);
    }
    ui.separator();
}

#[derive(Clone, Copy)]
enum FigureKind {
    CreateFlow,
    Reference,
    Inpaint,
    Video,
    Format,
    Generate,
    Chat,
    Canvas,
    Agents,
    Memory,
    Salon,
}

fn paint_figure(ui: &mut egui::Ui, kind: FigureKind) {
    let h = match kind {
        FigureKind::CreateFlow => 72.0,
        FigureKind::Video => 56.0,
        FigureKind::Format => 48.0,
        _ => 64.0,
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(520.0), h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let signal = Color32::from_rgb(62, 224, 196);
    let mute = ui.visuals().weak_text_color();
    let stroke = Stroke::new(1.2, signal);
    let faint = Stroke::new(1.0, mute);
    let p = ui.painter_at(rect);
    match kind {
        FigureKind::CreateFlow => {
            let box_w = rect.width() * 0.22;
            let img_w = rect.width() * 0.28;
            let prompt = Rect::from_min_size(rect.left_top() + Vec2::new(8.0, 16.0), Vec2::new(box_w, 40.0));
            let arrow1 = [prompt.right_center(), prompt.right_center() + Vec2::new(24.0, 0.0)];
            let engine = Rect::from_min_size(arrow1[1] + Vec2::new(4.0, -18.0), Vec2::new(box_w * 0.9, 36.0));
            let arrow2 = [engine.right_center(), engine.right_center() + Vec2::new(24.0, 0.0)];
            let out = Rect::from_min_size(arrow2[1] + Vec2::new(4.0, -22.0), Vec2::new(img_w, 44.0));
            p.rect_stroke(prompt, 2.0, stroke, egui::StrokeKind::Inside);
            p.rect_stroke(engine, 2.0, faint, egui::StrokeKind::Inside);
            p.rect_filled(out, 2.0, Color32::from_rgba_unmultiplied(62, 224, 196, 28));
            p.rect_stroke(out, 2.0, stroke, egui::StrokeKind::Inside);
            p.line_segment(arrow1, faint);
            p.line_segment(arrow2, faint);
        }
        FigureKind::Reference => {
            let src = Rect::from_min_size(rect.left_top() + Vec2::new(8.0, 10.0), Vec2::new(56.0, 44.0));
            let dst = Rect::from_min_size(rect.left_top() + Vec2::new(120.0, 8.0), Vec2::new(72.0, 48.0));
            p.rect_stroke(src, 2.0, faint, egui::StrokeKind::Inside);
            p.rect_filled(dst, 2.0, Color32::from_rgba_unmultiplied(62, 224, 196, 40));
            p.rect_stroke(dst, 2.0, stroke, egui::StrokeKind::Inside);
            p.line_segment([src.right_center(), dst.left_center()], stroke);
        }
        FigureKind::Inpaint => {
            let img = Rect::from_min_size(rect.center() - Vec2::new(60.0, 28.0), Vec2::new(120.0, 56.0));
            p.rect_stroke(img, 2.0, faint, egui::StrokeKind::Inside);
            let mask = Rect::from_min_size(img.left_top() + Vec2::new(70.0, 4.0), Vec2::new(44.0, 22.0));
            p.rect_filled(mask, 2.0, Color32::from_rgba_unmultiplied(232, 93, 76, 90));
            p.rect_stroke(mask, 2.0, Stroke::new(1.5, Color32::from_rgb(232, 93, 76)), egui::StrokeKind::Inside);
        }
        FigureKind::Video => {
            let mut x = rect.left() + 12.0;
            let y = rect.center().y - 18.0;
            for i in 0..4 {
                let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(28.0, 36.0));
                let alpha = 40 + i * 20;
                p.rect_filled(r, 2.0, Color32::from_rgba_unmultiplied(62, 224, 196, alpha));
                p.rect_stroke(r, 2.0, faint, egui::StrokeKind::Inside);
                x += 34.0;
            }
        }
        FigureKind::Format => {
            let small = Rect::from_min_size(rect.left_top() + Vec2::new(10.0, 14.0), Vec2::new(36.0, 36.0));
            let large = Rect::from_min_size(rect.left_top() + Vec2::new(60.0, 8.0), Vec2::new(52.0, 52.0));
            p.rect_stroke(small, 2.0, faint, egui::StrokeKind::Inside);
            p.rect_stroke(large, 2.0, stroke, egui::StrokeKind::Inside);
        }
        FigureKind::Generate => {
            let btn = Rect::from_min_size(rect.left_top() + Vec2::new(8.0, 20.0), Vec2::new(72.0, 28.0));
            p.rect_filled(btn, 3.0, Color32::from_rgba_unmultiplied(62, 224, 196, 50));
            p.rect_stroke(btn, 3.0, stroke, egui::StrokeKind::Inside);
            let preview = Rect::from_min_size(btn.right_top() + Vec2::new(20.0, -8.0), Vec2::new(64.0, 44.0));
            p.rect_filled(preview, 2.0, Color32::from_rgba_unmultiplied(62, 224, 196, 30));
            p.rect_stroke(preview, 2.0, stroke, egui::StrokeKind::Inside);
            p.line_segment([btn.right_center(), preview.left_center()], faint);
        }
        FigureKind::Chat => {
            let b1 = Rect::from_min_size(rect.left_top() + Vec2::new(8.0, 8.0), Vec2::new(100.0, 22.0));
            let b2 = Rect::from_min_size(rect.left_top() + Vec2::new(24.0, 36.0), Vec2::new(120.0, 22.0));
            p.rect_stroke(b1, 3.0, faint, egui::StrokeKind::Inside);
            p.rect_filled(b2, 3.0, Color32::from_rgba_unmultiplied(62, 224, 196, 35));
            p.rect_stroke(b2, 3.0, stroke, egui::StrokeKind::Inside);
        }
        FigureKind::Canvas => {
            let board = Rect::from_min_size(rect.center() - Vec2::new(70.0, 30.0), Vec2::new(140.0, 60.0));
            p.rect_stroke(board, 2.0, faint, egui::StrokeKind::Inside);
            p.line_segment(
                [board.left_top() + Vec2::new(12.0, 40.0), board.left_top() + Vec2::new(90.0, 18.0)],
                stroke,
            );
            p.circle_stroke(board.left_top() + Vec2::new(100.0, 38.0), 10.0, stroke);
        }
        FigureKind::Agents => {
            for (i, dx) in [(0.0_f32), (28.0), (56.0)].iter().enumerate() {
                let c = board_center(rect) + Vec2::new(dx - 28.0, 0.0);
                let col = if i == 1 { signal } else { mute };
                p.circle_stroke(c, 14.0, Stroke::new(1.2, col));
            }
        }
        FigureKind::Memory => {
            let chip = Rect::from_min_size(rect.left_top() + Vec2::new(12.0, 18.0), Vec2::new(88.0, 24.0));
            p.rect_filled(chip, 4.0, Color32::from_rgba_unmultiplied(62, 224, 196, 40));
            p.rect_stroke(chip, 4.0, stroke, egui::StrokeKind::Inside);
            let chip2 = chip.translate(Vec2::new(0.0, 30.0));
            p.rect_stroke(chip2, 4.0, faint, egui::StrokeKind::Inside);
        }
        FigureKind::Salon => {
            let y = rect.center().y;
            for (i, dx) in [0.0_f32, 36.0, 72.0].iter().enumerate() {
                let w = if i == 1 { 100.0 } else { 72.0 };
                let r = Rect::from_min_size(Pos2::new(rect.left() + 10.0 + dx, y - 12.0), Vec2::new(w, 24.0));
                if i == 1 {
                    p.rect_filled(r, 3.0, Color32::from_rgba_unmultiplied(62, 224, 196, 35));
                }
                p.rect_stroke(r, 3.0, if i == 1 { stroke } else { faint }, egui::StrokeKind::Inside);
            }
        }
    }
}

fn board_center(rect: Rect) -> Pos2 {
    rect.center()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_topics_cover_primary_surfaces() {
        assert_eq!(GuideTopic::ALL.len(), 7);
        assert_eq!(topic_for_tab(&Tab::Image), Some(GuideTopic::Create));
        assert_eq!(topic_for_tab(&Tab::Chat), Some(GuideTopic::Chat));
        assert!(topic_for_tab(&Tab::Settings).is_none());
    }

    #[test]
    fn guide_strings_en_fr_differ() {
        let en = strings("en");
        let fr = strings("fr");
        assert_ne!(en.nav_create, fr.nav_create);
        assert_ne!(en.create_prompt_body, fr.create_prompt_body);
        assert!(!en.create_intro.is_empty());
        assert!(!fr.salon_opt_turn_body.is_empty());
    }

    #[test]
    fn nav_labels_non_empty() {
        let g = strings("en");
        for topic in GuideTopic::ALL {
            assert!(!nav_label(&g, topic).is_empty());
        }
    }
}
