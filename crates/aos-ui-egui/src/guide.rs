//! In-app feature guides (EN/FR) — scrollable help surface, not a website or mill.

use crate::Tab;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GuideTopic {
    #[default]
    Overview,
    Create,
    Chat,
    Canvas,
    Agents,
    Memory,
    Library,
    Salon,
}

impl GuideTopic {
    pub const ALL: [GuideTopic; 8] = [
        GuideTopic::Overview,
        GuideTopic::Create,
        GuideTopic::Chat,
        GuideTopic::Canvas,
        GuideTopic::Agents,
        GuideTopic::Memory,
        GuideTopic::Library,
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
        Tab::Library => Some(GuideTopic::Library),
        _ => None,
    }
}

/// Small help affordance for tab/session headers (opens guide at `topic`).
pub fn tab_help_button(ui: &mut egui::Ui, tooltip: &str) -> bool {
    crate::icons::help_button(ui)
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
    pub nav_library: &'static str,
    pub nav_salon: &'static str,
    pub overview_intro: &'static str,
    pub overview_tour_chat: &'static str,
    pub overview_tour_create: &'static str,
    pub overview_tour_memory: &'static str,
    pub overview_tour_library: &'static str,
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
    pub create_negative_title: &'static str,
    pub create_negative_what: &'static str,
    pub create_negative_impact: &'static str,
    pub create_negative_example: &'static str,
    pub create_lora_title: &'static str,
    pub create_lora_what: &'static str,
    pub create_lora_impact: &'static str,
    pub create_lora_example: &'static str,
    pub create_vae_title: &'static str,
    pub create_vae_what: &'static str,
    pub create_vae_impact: &'static str,
    pub create_vae_example: &'static str,
    pub create_composition_title: &'static str,
    pub create_composition_what: &'static str,
    pub create_composition_impact: &'static str,
    pub create_composition_example: &'static str,
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
    pub chat_opt_sessions_example: &'static str,
    pub chat_opt_attach_title: &'static str,
    pub chat_opt_attach_body: &'static str,
    pub chat_opt_attach_example: &'static str,
    pub chat_opt_allow_once_title: &'static str,
    pub chat_opt_allow_once_body: &'static str,
    pub chat_opt_allow_once_example: &'static str,
    pub chat_opt_slash_title: &'static str,
    pub chat_opt_slash_body: &'static str,
    pub chat_opt_slash_example: &'static str,
    pub chat_opt_documents_title: &'static str,
    pub chat_opt_documents_body: &'static str,
    pub chat_opt_documents_example: &'static str,
    pub chat_schedule_heading: &'static str,
    pub chat_schedule_intro: &'static str,
    pub chat_schedule_card_title: &'static str,
    pub chat_schedule_card_body: &'static str,
    pub chat_schedule_paused_title: &'static str,
    pub chat_schedule_paused_body: &'static str,
    pub chat_schedule_stopped_title: &'static str,
    pub chat_schedule_stopped_body: &'static str,
    pub chat_schedule_example: &'static str,
    pub canvas_intro: &'static str,
    pub canvas_opt_toggle_title: &'static str,
    pub canvas_opt_toggle_body: &'static str,
    pub canvas_opt_toggle_example: &'static str,
    pub canvas_opt_tools_title: &'static str,
    pub canvas_opt_tools_body: &'static str,
    pub canvas_opt_tools_example: &'static str,
    pub canvas_opt_export_title: &'static str,
    pub canvas_opt_export_body: &'static str,
    pub canvas_opt_export_example: &'static str,
    pub agents_intro: &'static str,
    pub agents_opt_name_role_title: &'static str,
    pub agents_opt_name_role_body: &'static str,
    pub agents_opt_name_role_example: &'static str,
    pub agents_opt_tools_title: &'static str,
    pub agents_opt_tools_body: &'static str,
    pub agents_opt_tools_example: &'static str,
    pub agents_opt_history_title: &'static str,
    pub agents_opt_history_body: &'static str,
    pub agents_opt_history_example: &'static str,
    pub memory_intro: &'static str,
    pub memory_opt_remember_title: &'static str,
    pub memory_opt_remember_body: &'static str,
    pub memory_opt_remember_example: &'static str,
    pub memory_opt_recall_title: &'static str,
    pub memory_opt_recall_body: &'static str,
    pub memory_opt_recall_example: &'static str,
    pub memory_opt_list_title: &'static str,
    pub memory_opt_list_body: &'static str,
    pub memory_opt_list_example: &'static str,
    pub library_intro: &'static str,
    pub library_opt_add_title: &'static str,
    pub library_opt_add_body: &'static str,
    pub library_example: &'static str,
    pub salon_intro: &'static str,
    pub salon_opt_enable_title: &'static str,
    pub salon_opt_enable_body: &'static str,
    pub salon_opt_enable_example: &'static str,
    pub salon_opt_members_title: &'static str,
    pub salon_opt_members_body: &'static str,
    pub salon_opt_members_example: &'static str,
    pub salon_opt_turn_title: &'static str,
    pub salon_opt_turn_body: &'static str,
    pub salon_opt_turn_example: &'static str,
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
    nav_library: "Library",
    nav_salon: "Salon",
    overview_intro: "Short map of Preview surfaces. Pick a topic on the left for step-by-step help with examples.",
    overview_tour_chat: "Chat — parallel persisted conversations with the local model.",
    overview_tour_create: "Create — diffusion image and short video studio (prompt, reference, inpaint).",
    overview_tour_memory: "Memory — durable facts injected before each reply (remember / recall).",
    overview_tour_library: "Library — files you add; chat consults them when they match (More → Library).",
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
    create_negative_title: "Negative prompt",
    create_negative_what: "Things to keep out of the still.",
    create_negative_impact: "Generate reads this as a \"don't\". Empty = nothing excluded.",
    create_negative_example: "Ex. \"text, watermark, extra fingers\" → fewer letters and extra limbs.",
    create_lora_title: "LoRA",
    create_lora_what: "A look or character from the model's list (not a file path).",
    create_lora_impact: "Turns on that extra style for this run.",
    create_lora_example: "Ex. Pick a \"watercolor\" LoRA + mug prompt → same subject, painted look.",
    create_vae_title: "VAE",
    create_vae_what: "How the still is decoded. Default is the pack's.",
    create_vae_impact: "Another VAE from the list can change color and smoothness. Leave default if unsure.",
    create_vae_example: "Ex. Keep default unless a listed VAE is meant for this pack.",
    create_composition_title: "Composition",
    create_composition_what: "Overlapping blocks on the result canvas.",
    create_composition_impact: "Generate uses their layout so objects sit where you placed them. Fix a region is painted on this same view.",
    create_composition_example: "Ex. Block left = mug, block right = window → two subjects, that arrangement.",
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
    chat_intro: "A place for one conversation at a time. Each session keeps its own thread, model, Canvas and Salon toggles.",
    chat_opt_sessions_title: "Sessions",
    chat_opt_sessions_body: "Switch or start a session in the sidebar. History survives a restart.",
    chat_opt_sessions_example: "Ex. Two sessions: one for a trip, one for a recipe — they don't mix.",
    chat_opt_attach_title: "Paperclip",
    chat_opt_attach_body: "Attach image (vision models) or Attach document (PDF, txt, md). Chips go with the next message.",
    chat_opt_attach_example: "Ex. PDF of a menu + \"what's vegetarian?\" → an answer from that file.",
    chat_opt_allow_once_title: "Allow once",
    chat_opt_allow_once_body: "When an agent wants to act, the thread asks. Allow once does it this time; Always allow remembers.",
    chat_opt_allow_once_example: "Ex. The agent wants to draw → Allow once → one stroke, then it asks again.",
    chat_opt_slash_title: "Slash",
    chat_opt_slash_body: "Type / above the input for shortcuts without leaving Chat (/help, /notes, /agent, /canvas).",
    chat_opt_slash_example: "Ex. `/agent summarize this thread` starts a worker; a card appears in the session.",
    chat_opt_documents_title: "Prepared documents",
    chat_opt_documents_body: "On a research-style question, choose Reply for a short answer in chat, or Prepare a document for a structured file under /downloads/. While it runs you see Researching… with Stop; when ready, Open shows the document in a clamped overlay. Find prepared files anytime under More → Documents.",
    chat_opt_documents_example: "\"What's the state of agentic apps?\" → Prepare a document → Researching… → Ready → Open, sources at the bottom.",
    chat_schedule_heading: "Schedule",
    chat_schedule_intro: "You give a goal and a when. After Allow once, it comes back without you reopening the session. Settings still holds the list.",
    chat_schedule_card_title: "Card",
    chat_schedule_card_body: "Title is your phrase. One mute line: Next: tomorrow 8:00. Pause | Stop.",
    chat_schedule_paused_title: "Paused",
    chat_schedule_paused_body: "Line becomes Paused. Resume | Stop.",
    chat_schedule_stopped_title: "Stopped",
    chat_schedule_stopped_body: "Card stays in the thread. No more actions.",
    chat_schedule_example: "Ex. \"every morning, summarize my notes\" → Allow once → card in the thread, next fire tomorrow.",
    canvas_intro: "A shared drawing board on this chat session. Strokes, not a Create still.",
    canvas_opt_toggle_title: "Canvas toggle",
    canvas_opt_toggle_body: "Session bar → Canvas. Humans can always draw. Agents draw only while it's open (otherwise \"draw\" goes to Create).",
    canvas_opt_toggle_example: "Ex. Canvas on + \"draw a mug\" → strokes on the board. Canvas off + \"draw a mug\" → a generated picture.",
    canvas_opt_tools_title: "Tools",
    canvas_opt_tools_body: "Pen, Eraser, Line, Spline, Rect, Ellipse, Bucket; Tint, Width, Fill. Undo my stroke; Clear asks first. Format: Square, 16:9, 16:10, Vertical, Horizontal.",
    canvas_opt_tools_example: "Ex. Ellipse + Fill + a teal Tint → a solid mug body.",
    canvas_opt_export_title: "Export PNG",
    canvas_opt_export_body: "Snapshot of the board into Downloads. Same scene, pixels you can share. Not Generate.",
    canvas_opt_export_example: "Ex. Export PNG after a sketch → a file, the board stays.",
    agents_intro: "A library of people you can reuse. Creating one does not start a run. The Name is what shows in the thread and the Salon picker (one row each).",
    agents_opt_name_role_title: "Name & role",
    agents_opt_name_role_body: "Name is required. Role is optional: how they behave in a Salon. Launch a run with /agent in Chat, or Add to session.",
    agents_opt_name_role_example: "Ex. Name \"Maya\", role \"be brief\" → the thread says Maya, not a generic id.",
    agents_opt_tools_title: "Tools",
    agents_opt_tools_body: "Grant only what they need. Sensitive steps still ask in the thread (Allow once).",
    agents_opt_tools_example: "Ex. Canvas tools on + Salon → Maya can draw after you allow it.",
    agents_opt_history_title: "History",
    agents_opt_history_body: "Open a past run: steps, Pause / Resume / Retry / Kill / Steer.",
    agents_opt_history_example: "Ex. Kill stops a loop; Steer adds a note mid-run.",
    memory_intro: "Short facts about you, reused next time. Not the transcript, not a one-shot sketch.",
    memory_opt_remember_title: "Remember",
    memory_opt_remember_body: "Save a fact (or let Auto-remember from chat, on by default). Secrets are not stored.",
    memory_opt_remember_example: "Ex. \"I take tea, not coffee\" → it stops offering coffee.",
    memory_opt_recall_title: "Recall",
    memory_opt_recall_body: "Recall, hint \"recall query\". Finds a stored fact.",
    memory_opt_recall_example: "Ex. Recall \"tea\" → that preference, not last week's canvas doodle.",
    memory_opt_list_title: "List",
    memory_opt_list_body: "Edit or drop facts. A quiet daily pass re-reads the day's chats and links related facts.",
    memory_opt_list_example: "Ex. Two \"name\" facts → the newer one updates the older.",
    library_intro: "Files you add. Chat uses them when they match. If they don't, it still answers — nothing about a miss in the thread.",
    library_opt_add_title: "Add",
    library_opt_add_body: "From More → Library, pick pdf, txt, or md. Each file is listed by title and date — never a full path.",
    library_example: "Ex. House-rules PDF + \"what's the quiet hour?\" → an answer from that file. \"What's a bubble wand?\" → an answer anyway.",
    salon_intro: "Several agents in the same Chat session, taking turns. Not an outside messenger.",
    salon_opt_enable_title: "Enable room",
    salon_opt_enable_body: "Session bar → Salon. The thread becomes multi-speaker.",
    salon_opt_enable_example: "Ex. Salon on → Members strip appears; 1:1 chat becomes a room.",
    salon_opt_members_title: "Members",
    salon_opt_members_body: "One list: built-in personas and your library agents, each Name once. Add from library. @ only real members.",
    salon_opt_members_example: "Ex. Add Maya + Critic → two names in the strip, no duplicate row.",
    salon_opt_turn_title: "Room turn",
    salon_opt_turn_body: "Without @, everyone in the strip answers in order (up to 4). @ still picks who speaks. Cancel stops the turn.",
    salon_opt_turn_example: "Ex. \"Review this sketch\" → Maya then Critic, same thread.",
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
    nav_library: "Bibliothèque",
    nav_salon: "Salon",
    overview_intro: "Carte rapide des surfaces Preview. Choisissez un sujet à gauche pour une aide pas à pas avec exemples.",
    overview_tour_chat: "Chat — conversations parallèles persistées avec le modèle local.",
    overview_tour_create: "Créer — studio diffusion images et courtes vidéos (prompt, référence, inpaint).",
    overview_tour_memory: "Mémoire — faits durables injectés avant chaque réponse (remember / recall).",
    overview_tour_library: "Bibliothèque — fichiers que vous ajoutez ; le chat s'en sert s'ils collent (Plus → Bibliothèque).",
    overview_tour_agents: "Agents — boucles autonomes avec outils, skills et capacités.",
    overview_tour_canvas: "Canvas — dessin vectoriel partagé sur la session de chat active (pas les pixels Créer).",
    overview_tour_salon: "Salon — salle multi-agents dans une session de chat (personas intégrés + agents bibliothèque).",
    section_what: "À quoi ça sert",
    section_impact: "Ce que ça change",
    section_example: "Exemple",
    create_intro: "Créer transforme une description en image, ou en court clip. Vous pouvez partir d'une photo déjà là, ou peindre une zone du résultat.",
    create_prompt_title: "Prompt",
    create_prompt_body: "Dites ce qui doit apparaître : sujet, lieu, lumière. Générer lit ce texte. Le précis bat le vague.",
    create_prompt_example: "Ex. « Une tasse bleue sur un bureau en bois, lumière du matin » → un objet, lumière chaude.",
    create_negative_title: "Prompt négatif",
    create_negative_what: "Ce qu'il ne faut pas dans l'image.",
    create_negative_impact: "Générer lit ça comme un « sans ». Vide = rien d'exclu.",
    create_negative_example: "Ex. « texte, filigrane, doigts en trop » → moins de lettres et de membres en trop.",
    create_lora_title: "LoRA",
    create_lora_what: "Un look ou un personnage, dans la liste du modèle (pas un chemin de fichier).",
    create_lora_impact: "Active ce style pour ce run.",
    create_lora_example: "Ex. LoRA « aquarelle » + tasse → même sujet, rendu peint.",
    create_vae_title: "VAE",
    create_vae_what: "Comment l'image est décodée. Défaut = celui du pack.",
    create_vae_impact: "Un autre VAE de la liste peut changer couleur et douceur. Laissez le défaut si vous n'êtes pas sûr.",
    create_vae_example: "Ex. Gardez le défaut sauf si un VAE listé va avec ce pack.",
    create_composition_title: "Composition",
    create_composition_what: "Blocs qui se chevauchent sur le canevas du résultat.",
    create_composition_impact: "Générer s'en sert pour placer les objets. Corriger une zone se peint sur la même vue.",
    create_composition_example: "Ex. Bloc gauche = tasse, bloc droit = fenêtre → deux sujets, cet arrangement.",
    create_reference_title: "Image de référence",
    create_reference_body: "Trombone à côté du prompt. Créer part de cette photo. Plus près de la photo : pose et couleurs. Plus loin : le prompt gagne.",
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
    create_generate_body: "Lance Créer avec le prompt et les options du moment. Il faut un modèle image ou vidéo installé. Le résultat s'affiche à droite ; l'historique garde les précédents.",
    create_generate_example: "",
    chat_intro: "Une conversation à la fois. Chaque session garde son fil, son modèle, Canvas et Salon.",
    chat_opt_sessions_title: "Sessions",
    chat_opt_sessions_body: "Créez ou changez de session dans la barre. L'historique survit au redémarrage.",
    chat_opt_sessions_example: "Ex. Deux sessions : un voyage, une recette — ça ne se mélange pas.",
    chat_opt_attach_title: "Trombone",
    chat_opt_attach_body: "Joindre une image (modèles vision) ou Joindre un document (PDF, txt, md). Les puces partent avec le prochain message.",
    chat_opt_attach_example: "Ex. PDF d'un menu + « c'est quoi le végétarien ? » → une réponse à partir du fichier.",
    chat_opt_allow_once_title: "Autoriser une fois",
    chat_opt_allow_once_body: "Quand un agent veut agir, le fil demande. Autoriser une fois = cette fois ; Toujours autoriser (Demander dans la barre) = il s'en souvient.",
    chat_opt_allow_once_example: "Ex. L'agent veut tracer → Autoriser une fois → un trait, puis il redemande.",
    chat_opt_slash_title: "Slash",
    chat_opt_slash_body: "Tapez / au-dessus de l'input (/help, /notes, /agent, /canvas).",
    chat_opt_slash_example: "Ex. `/agent résume ce fil` lance un worker ; une carte apparaît.",
    chat_opt_documents_title: "Documents préparés",
    chat_opt_documents_body: "Sur une question de recherche, choisissez Répondre pour une courte réponse dans le chat, ou Préparer un document pour un fichier structuré sous /downloads/. Pendant la préparation : Recherche en cours… avec Arrêter ; une fois prêt, Ouvrir affiche le document dans une fenêtre calée au viewport. Retrouvez les fichiers via Plus → Documents.",
    chat_opt_documents_example: "« Quel est l'état de l'art des apps agentic ? » → Préparer un document → Recherche en cours… → Prêt → Ouvrir, sources en bas.",
    chat_schedule_heading: "Planifier",
    chat_schedule_intro: "Vous donnez un but et un quand. Après Autoriser une fois, ça revient sans rouvrir la session. La liste reste dans Réglages.",
    chat_schedule_card_title: "Carte",
    chat_schedule_card_body: "Le titre est votre phrase. Une ligne mute : Prochain : demain 8:00. Pause | Arrêter.",
    chat_schedule_paused_title: "En pause",
    chat_schedule_paused_body: "La ligne devient En pause. Reprendre | Arrêter.",
    chat_schedule_stopped_title: "Arrêté",
    chat_schedule_stopped_body: "La carte reste dans le fil. Plus d'actions.",
    chat_schedule_example: "Ex. « chaque matin, résume mes notes » → Autoriser une fois → carte dans le fil, prochain feu demain.",
    canvas_intro: "Un tableau partagé sur cette session. Des traits, pas une image Créer.",
    canvas_opt_toggle_title: "Canvas",
    canvas_opt_toggle_body: "Barre de session → Canvas. Vous dessinez toujours. Les agents, seulement s'il est ouvert (sinon « dessine » va vers Créer).",
    canvas_opt_toggle_example: "Ex. Canvas ouvert + « dessine une tasse » → des traits. Fermé + « dessine une tasse » → une image générée.",
    canvas_opt_tools_title: "Outils",
    canvas_opt_tools_body: "Crayon, Gomme, Ligne, Courbe, Rectangle, Ellipse, Seau ; Teinte, Épaisseur, Plein. Annuler mon trait ; Tout effacer demande confirmation. Format : Carré, 16:9, 16:10, Vertical, Horizontal.",
    canvas_opt_tools_example: "Ex. Ellipse + Plein + teinte bleu-vert → le corps de la tasse.",
    canvas_opt_export_title: "Exporter PNG",
    canvas_opt_export_body: "Instantané du tableau dans Téléchargements. La même scène, en pixels. Pas Générer.",
    canvas_opt_export_example: "Ex. Exporter PNG après un croquis → un fichier, le tableau reste.",
    agents_intro: "Une bibliothèque. Créer ne lance pas de run. Le Nom s'affiche dans le fil et dans le sélecteur Salon (une ligne chacun).",
    agents_opt_name_role_title: "Nom et rôle",
    agents_opt_name_role_body: "Nom obligatoire. Rôle optionnel : comportement en salon. Lancer un run : `/agent` dans le Chat, ou Ajouter à la session.",
    agents_opt_name_role_example: "Ex. Nom « Maya », rôle « sois brève » → le fil dit Maya.",
    agents_opt_tools_title: "Outils",
    agents_opt_tools_body: "Accordez seulement ce qu'il faut. Les gestes sensibles demandent dans le fil (Autoriser une fois).",
    agents_opt_tools_example: "Ex. Outils Canvas + Salon → Maya peut tracer après votre OK.",
    agents_opt_history_title: "Historique",
    agents_opt_history_body: "Ouvrir un run : étapes, Pause / Débloquer / Relancer / Kill / Orienter.",
    agents_opt_history_example: "Ex. Kill coupe une boucle ; Orienter ajoute une note en cours.",
    memory_intro: "De courts faits sur vous, réutilisés plus tard. Pas le transcript, pas un croquis one-shot.",
    memory_opt_remember_title: "Mémoriser",
    memory_opt_remember_body: "Enregistrer un fait (ou Mémorisation auto depuis le chat, activée par défaut). Pas de secrets.",
    memory_opt_remember_example: "Ex. « Je prends du thé, pas de café » → plus de café proposé.",
    memory_opt_recall_title: "Rappeler",
    memory_opt_recall_body: "Bouton Rappeler, hint « chercher un fait ». Retrouve un fait stocké.",
    memory_opt_recall_example: "Ex. Rappeler « thé » → cette préférence, pas le doodle du canvas.",
    memory_opt_list_title: "Liste",
    memory_opt_list_body: "Modifier ou retirer. Une passe silencieuse relit les chats du jour et relie les faits.",
    memory_opt_list_example: "Ex. Deux faits « prénom » → le plus récent met à jour l'ancien.",
    library_intro: "Des fichiers que vous ajoutez. Le chat s'en sert s'ils collent. Sinon il répond quand même — rien sur un miss dans le fil.",
    library_opt_add_title: "Ajouter",
    library_opt_add_body: "Depuis Plus → Bibliothèque, choisissez pdf, txt ou md. Chaque fichier est listé par titre et date — jamais un chemin complet.",
    library_example: "Ex. PDF du règlement + « c'est quoi l'heure calme ? » → une réponse à partir du fichier. « C'est quoi un bubble wand ? » → une réponse quand même.",
    salon_intro: "Plusieurs agents dans la même session Chat, à tour de rôle. Pas une messagerie externe.",
    salon_opt_enable_title: "Activer le salon",
    salon_opt_enable_body: "Barre de session → Salon. Le fil devient multi-voix.",
    salon_opt_enable_example: "Ex. Salon on → bande Membres ; le 1:1 devient une salle.",
    salon_opt_members_title: "Membres",
    salon_opt_members_body: "Une seule liste : personas intégrés et agents de la bibliothèque, chaque Nom une fois. Ajouter depuis la bibliothèque. @ seulement les vrais membres.",
    salon_opt_members_example: "Ex. Maya + Critique → deux noms dans la bande, pas de doublon.",
    salon_opt_turn_title: "Tour de salon",
    salon_opt_turn_body: "Sans `@`, tous les membres de la bande répondent dans l'ordre (plafond 4). `@` choisit qui parle. Annuler stoppe le tour.",
    salon_opt_turn_example: "Ex. « Critique ce croquis » → Maya puis Critique, dans le même fil.",
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
        GuideTopic::Library => g.nav_library,
        GuideTopic::Salon => g.nav_salon,
    }
}

pub(crate) fn guide_window_sizes(avail_w: f32, avail_h: f32) -> ([f32; 2], [f32; 2], [f32; 2]) {
    let margin = 16.0;
    let max_w = (avail_w - margin * 2.0).max(280.0);
    let max_h = (avail_h - margin * 2.0).max(200.0);
    let default_size = [820.0_f32.min(max_w), 560.0_f32.min(max_h)];
    let max_size = [max_w, max_h];
    let min_size = [280.0_f32.min(max_w), 200.0_f32.min(max_h)];
    (default_size, max_size, min_size)
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
    let avail = ctx.available_rect();
    let (default_size, max_size, min_size) =
        guide_window_sizes(avail.width(), avail.height());
    const NAV_W: f32 = 148.0;
    const FOOTER_H: f32 = 34.0;

    egui::Window::new(g.title)
        .collapsible(false)
        .resizable(true)
        .default_size(default_size)
        .min_size(min_size)
        .max_size(max_size)
        .constrain(true)
        .constrain_to(avail)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            let body_h = (ui.available_height() - FOOTER_H).max(80.0);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), body_h),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.horizontal_top(|ui| {
                        let nav_h = ui.available_height().max(80.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(NAV_W, nav_h),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(NAV_W);
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
                        let content_w = ui.available_width().max(120.0);
                        let content_h = ui.available_height().max(80.0);
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
                                        ui.set_min_width((content_w - 8.0).max(100.0));
                                        render_topic(ui, state.topic, &g);
                                    });
                            },
                        );
                    });
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), FOOTER_H),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    if ui.button(g.close).clicked() {
                        close = true;
                    }
                },
            );
        });
    if close {
        state.open = false;
    }
}

fn render_topic(ui: &mut egui::Ui, topic: GuideTopic, g: &GuideStrings) {
    match topic {
        GuideTopic::Overview => render_overview(ui, g),
        GuideTopic::Create => render_create(ui, g),
        GuideTopic::Chat => render_chat(ui, g),
        GuideTopic::Canvas => render_canvas(ui, g),
        GuideTopic::Agents => render_agents(ui, g),
        GuideTopic::Memory => render_memory(ui, g),
        GuideTopic::Library => render_library(ui, g),
        GuideTopic::Salon => render_salon(ui, g),
    }
}

fn render_library(ui: &mut egui::Ui, g: &GuideStrings) {
    ui.heading(g.nav_library);
    ui.label(g.library_intro);
    ui.add_space(8.0);
    guide_section(
        ui,
        g,
        g.library_opt_add_title,
        g.library_opt_add_body,
        None,
        None,
        g.library_example,
    );
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
        g.overview_tour_library,
    ] {
        ui.label(line);
    }
}

fn render_chat(ui: &mut egui::Ui, g: &GuideStrings) {
    ui.heading(g.nav_chat);
    ui.label(g.chat_intro);
    ui.add_space(8.0);
    paint_figure(ui, FigureKind::Chat);
    ui.add_space(8.0);
    guide_section(
        ui,
        g,
        g.chat_opt_sessions_title,
        g.chat_opt_sessions_body,
        None,
        None,
        g.chat_opt_sessions_example,
    );
    guide_section(
        ui,
        g,
        g.chat_opt_attach_title,
        g.chat_opt_attach_body,
        None,
        None,
        g.chat_opt_attach_example,
    );
    guide_section(
        ui,
        g,
        g.chat_opt_allow_once_title,
        g.chat_opt_allow_once_body,
        None,
        None,
        g.chat_opt_allow_once_example,
    );
    guide_section(
        ui,
        g,
        g.chat_opt_slash_title,
        g.chat_opt_slash_body,
        None,
        None,
        g.chat_opt_slash_example,
    );
    guide_section(
        ui,
        g,
        g.chat_opt_documents_title,
        g.chat_opt_documents_body,
        None,
        None,
        g.chat_opt_documents_example,
    );
    ui.add_space(4.0);
    ui.strong(g.chat_schedule_heading);
    ui.label(g.chat_schedule_intro);
    ui.add_space(6.0);
    for (title, body) in [
        (g.chat_schedule_card_title, g.chat_schedule_card_body),
        (g.chat_schedule_paused_title, g.chat_schedule_paused_body),
        (g.chat_schedule_stopped_title, g.chat_schedule_stopped_body),
    ] {
        ui.strong(title);
        ui.label(body);
        ui.add_space(6.0);
    }
    ui.weak(g.section_example);
    ui.label(g.chat_schedule_example);
    ui.separator();
}

fn render_canvas(ui: &mut egui::Ui, g: &GuideStrings) {
    ui.heading(g.nav_canvas);
    ui.label(g.canvas_intro);
    ui.add_space(8.0);
    paint_figure(ui, FigureKind::Canvas);
    ui.add_space(8.0);
    guide_section(
        ui,
        g,
        g.canvas_opt_toggle_title,
        g.canvas_opt_toggle_body,
        None,
        None,
        g.canvas_opt_toggle_example,
    );
    guide_section(
        ui,
        g,
        g.canvas_opt_tools_title,
        g.canvas_opt_tools_body,
        None,
        None,
        g.canvas_opt_tools_example,
    );
    guide_section(
        ui,
        g,
        g.canvas_opt_export_title,
        g.canvas_opt_export_body,
        None,
        None,
        g.canvas_opt_export_example,
    );
}

fn render_agents(ui: &mut egui::Ui, g: &GuideStrings) {
    ui.heading(g.nav_agents);
    ui.label(g.agents_intro);
    ui.add_space(8.0);
    paint_figure(ui, FigureKind::Agents);
    ui.add_space(8.0);
    guide_section(
        ui,
        g,
        g.agents_opt_name_role_title,
        g.agents_opt_name_role_body,
        None,
        None,
        g.agents_opt_name_role_example,
    );
    guide_section(
        ui,
        g,
        g.agents_opt_tools_title,
        g.agents_opt_tools_body,
        None,
        None,
        g.agents_opt_tools_example,
    );
    guide_section(
        ui,
        g,
        g.agents_opt_history_title,
        g.agents_opt_history_body,
        None,
        None,
        g.agents_opt_history_example,
    );
}

fn render_memory(ui: &mut egui::Ui, g: &GuideStrings) {
    ui.heading(g.nav_memory);
    ui.label(g.memory_intro);
    ui.add_space(8.0);
    paint_figure(ui, FigureKind::Memory);
    ui.add_space(8.0);
    guide_section(
        ui,
        g,
        g.memory_opt_remember_title,
        g.memory_opt_remember_body,
        None,
        None,
        g.memory_opt_remember_example,
    );
    guide_section(
        ui,
        g,
        g.memory_opt_recall_title,
        g.memory_opt_recall_body,
        None,
        None,
        g.memory_opt_recall_example,
    );
    guide_section(
        ui,
        g,
        g.memory_opt_list_title,
        g.memory_opt_list_body,
        None,
        None,
        g.memory_opt_list_example,
    );
}

fn render_salon(ui: &mut egui::Ui, g: &GuideStrings) {
    ui.heading(g.nav_salon);
    ui.label(g.salon_intro);
    ui.add_space(8.0);
    paint_figure(ui, FigureKind::Salon);
    ui.add_space(8.0);
    guide_section(
        ui,
        g,
        g.salon_opt_enable_title,
        g.salon_opt_enable_body,
        None,
        None,
        g.salon_opt_enable_example,
    );
    guide_section(
        ui,
        g,
        g.salon_opt_members_title,
        g.salon_opt_members_body,
        None,
        None,
        g.salon_opt_members_example,
    );
    guide_section(
        ui,
        g,
        g.salon_opt_turn_title,
        g.salon_opt_turn_body,
        None,
        None,
        g.salon_opt_turn_example,
    );
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
        None,
        g.create_prompt_example,
    );
    guide_section(
        ui,
        g,
        g.create_negative_title,
        g.create_negative_what,
        Some(g.create_negative_impact),
        Some(FigureKind::Negative),
        g.create_negative_example,
    );
    guide_section(
        ui,
        g,
        g.create_lora_title,
        g.create_lora_what,
        Some(g.create_lora_impact),
        Some(FigureKind::Lora),
        g.create_lora_example,
    );
    guide_section(
        ui,
        g,
        g.create_vae_title,
        g.create_vae_what,
        Some(g.create_vae_impact),
        Some(FigureKind::Vae),
        g.create_vae_example,
    );
    guide_section(
        ui,
        g,
        g.create_composition_title,
        g.create_composition_what,
        Some(g.create_composition_impact),
        Some(FigureKind::Composition),
        g.create_composition_example,
    );
    guide_section(
        ui,
        g,
        g.create_reference_title,
        g.create_reference_body,
        None,
        Some(FigureKind::Reference),
        g.create_reference_example,
    );
    guide_section(
        ui,
        g,
        g.create_inpaint_title,
        g.create_inpaint_body,
        None,
        Some(FigureKind::Inpaint),
        g.create_inpaint_example,
    );
    guide_section(
        ui,
        g,
        g.create_video_title,
        g.create_video_body,
        None,
        Some(FigureKind::Video),
        g.create_video_example,
    );
    guide_section(
        ui,
        g,
        g.create_format_title,
        g.create_format_body,
        None,
        Some(FigureKind::Format),
        g.create_format_example,
    );
    guide_section(
        ui,
        g,
        g.create_generate_title,
        g.create_generate_body,
        None,
        Some(FigureKind::Generate),
        g.create_generate_example,
    );
}

fn guide_section(
    ui: &mut egui::Ui,
    g: &GuideStrings,
    title: &str,
    what: &str,
    impact: Option<&str>,
    figure: Option<FigureKind>,
    example: &str,
) {
    ui.add_space(4.0);
    ui.strong(title);
    ui.weak(g.section_what);
    ui.label(what);
    if let Some(impact) = impact.filter(|s| !s.is_empty()) {
        ui.weak(g.section_impact);
        ui.label(impact);
    }
    if let Some(kind) = figure {
        ui.add_space(4.0);
        paint_figure(ui, kind);
    }
    if !example.is_empty() {
        ui.weak(g.section_example);
        ui.label(example);
    }
    ui.separator();
}

#[derive(Clone, Copy)]
enum FigureKind {
    CreateFlow,
    Negative,
    Lora,
    Vae,
    Composition,
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
    let stroke = Stroke::new(1.2_f32, signal);
    let faint = Stroke::new(1.0_f32, mute);
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
        FigureKind::Negative => {
            let field = Rect::from_min_size(rect.left_top() + Vec2::new(8.0, 18.0), Vec2::new(140.0, 28.0));
            p.rect_stroke(field, 2.0, faint, egui::StrokeKind::Inside);
            let c = field.center();
            let r = field.width() * 0.28;
            p.line_segment([c + Vec2::new(-r, -r * 0.2), c + Vec2::new(r, r * 0.2)], Stroke::new(1.4_f32, Color32::from_rgb(232, 93, 76)));
        }
        FigureKind::Lora => {
            let base = Rect::from_min_size(rect.left_top() + Vec2::new(8.0, 12.0), Vec2::new(72.0, 40.0));
            let addon = Rect::from_min_size(base.right_top() + Vec2::new(6.0, 8.0), Vec2::new(36.0, 24.0));
            p.rect_stroke(base, 2.0, faint, egui::StrokeKind::Inside);
            p.rect_filled(addon, 2.0, Color32::from_rgba_unmultiplied(62, 224, 196, 45));
            p.rect_stroke(addon, 2.0, stroke, egui::StrokeKind::Inside);
        }
        FigureKind::Vae => {
            let block = Rect::from_min_size(rect.left_top() + Vec2::new(8.0, 14.0), Vec2::new(52.0, 36.0));
            let out = Rect::from_min_size(block.right_top() + Vec2::new(16.0, -4.0), Vec2::new(64.0, 44.0));
            p.rect_stroke(block, 2.0, faint, egui::StrokeKind::Inside);
            p.rect_filled(out, 2.0, Color32::from_rgba_unmultiplied(62, 224, 196, 35));
            p.rect_stroke(out, 2.0, stroke, egui::StrokeKind::Inside);
            p.line_segment([block.right_center(), out.left_center()], faint);
        }
        FigureKind::Composition => {
            let b1 = Rect::from_min_size(rect.left_top() + Vec2::new(10.0, 24.0), Vec2::new(56.0, 36.0));
            let b2 = Rect::from_min_size(rect.left_top() + Vec2::new(34.0, 8.0), Vec2::new(48.0, 28.0));
            let b3 = Rect::from_min_size(rect.left_top() + Vec2::new(78.0, 18.0), Vec2::new(40.0, 22.0));
            p.rect_stroke(b1, 2.0, faint, egui::StrokeKind::Inside);
            p.rect_filled(b2, 2.0, Color32::from_rgba_unmultiplied(62, 224, 196, 40));
            p.rect_stroke(b2, 2.0, stroke, egui::StrokeKind::Inside);
            p.rect_stroke(b3, 2.0, faint, egui::StrokeKind::Inside);
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
            p.rect_stroke(mask, 2.0, Stroke::new(1.5_f32, Color32::from_rgb(232, 93, 76)), egui::StrokeKind::Inside);
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
                p.circle_stroke(c, 14.0, Stroke::new(1.2_f32, col));
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
    fn guide_window_sizes_fit_inside_viewport() {
        let (default_size, max_size, min_size) = guide_window_sizes(640.0, 480.0);
        assert!(default_size[0] <= max_size[0]);
        assert!(default_size[1] <= max_size[1]);
        assert!(min_size[0] <= max_size[0]);
        assert!(min_size[1] <= max_size[1]);
        assert!(default_size[0] <= 640.0);
        assert!(default_size[1] <= 480.0);
    }

    #[test]
    fn guide_topics_cover_primary_surfaces() {
        assert_eq!(GuideTopic::ALL.len(), 8);
        assert_eq!(topic_for_tab(&Tab::Image), Some(GuideTopic::Create));
        assert_eq!(topic_for_tab(&Tab::Chat), Some(GuideTopic::Chat));
        assert_eq!(topic_for_tab(&Tab::Library), Some(GuideTopic::Library));
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

    #[test]
    fn create_guide_covers_studio_controls() {
        let en = strings("en");
        let fr = strings("fr");
        assert_eq!(en.create_negative_title, "Negative prompt");
        assert_eq!(fr.create_inpaint_title, "Corriger une zone");
        assert!(fr.create_intro.starts_with("Créer transforme"));
        assert!(!fr.create_intro.contains("Create"));
        assert!(!fr.create_reference_body.contains("Create"));
        assert!(!fr.create_generate_body.contains("Create"));
        assert!(!en.create_lora_what.is_empty());
        assert!(!fr.create_composition_impact.is_empty());
        assert!(en.create_lora_what.contains("model's list"));
        assert!(!en.create_lora_what.contains("share/models"));
        assert!(!fr.create_composition_impact.to_lowercase().contains("inpaint"));
    }

    #[test]
    fn chat_guide_has_four_sections_including_allow_once() {
        let en = strings("en");
        let fr = strings("fr");
        assert_eq!(en.chat_opt_allow_once_title, "Allow once");
        assert_eq!(fr.chat_opt_allow_once_title, "Autoriser une fois");
        assert!(en.chat_intro.contains("one conversation at a time"));
        assert!(!en.chat_opt_slash_body.contains("platform tools"));
        assert!(!en.chat_opt_attach_example.is_empty());
        assert!(!fr.chat_opt_allow_once_example.is_empty());
        assert!(!en.chat_opt_allow_once_body.contains("Toujours autoriser"));
        assert!(!en.salon_opt_enable_body.contains("Activer le salon"));
    }

    #[test]
    fn agents_guide_name_role_not_goal() {
        let en = strings("en");
        let fr = strings("fr");
        assert_eq!(en.agents_opt_name_role_title, "Name & role");
        assert_eq!(fr.agents_opt_name_role_title, "Nom et rôle");
        assert!(fr.agents_intro.contains("Créer ne lance pas de run"));
        assert!(!fr.agents_intro.contains("n'lance"));
        assert!(!en.agents_opt_tools_body.contains("MCP"));
        assert!(!en.agents_intro.contains("goal"));
        assert!(fr.agents_opt_history_body.contains("Débloquer"));
        assert!(!fr.agents_opt_history_body.contains("Reprendre"));
        assert!(!fr.agents_opt_history_body.contains("Arrêter"));
    }

    #[test]
    fn surface_guides_use_examples_not_stub_jargon() {
        let en = strings("en");
        let fr = strings("fr");
        assert!(!en.canvas_opt_export_body.contains("/downloads"));
        assert!(!fr.canvas_opt_export_body.contains("/downloads"));
        assert!(!en.canvas_opt_export_body.contains("media.image.generate"));
        assert!(!en.memory_opt_recall_body.to_lowercase().contains("semantic"));
        assert!(!en.salon_opt_turn_body.contains("conductor"));
        assert!(!en.salon_opt_turn_body.contains("Your message runs a turn"));
        assert!(en.salon_opt_turn_body.contains("up to 4"));
        assert!(!fr.salon_opt_turn_body.contains("Votre message lance un tour"));
        assert!(fr.salon_opt_turn_body.contains("plafond 4"));
        assert!(!en.canvas_opt_export_example.is_empty());
        assert!(!fr.salon_opt_turn_example.is_empty());
    }
}
