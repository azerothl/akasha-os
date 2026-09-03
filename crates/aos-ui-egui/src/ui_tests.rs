//! Unit tests for the application shell and extracted UI modules.

use super::*;

#[cfg(test)]
mod delegate_tests {
    use super::*;
    use crate::chat_delegate::canvas_model_id;
    use aos_proto::{CanvasAspect, ModelInfo, ModelState};

    const ASPECT: CanvasAspect = CanvasAspect::Square;

    fn full_canvas_exported() -> Vec<String> {
        aos_agent::tools::CANVAS_TOOL_IDS
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    fn model(id: &str, state: ModelState, has_vision: bool) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            name: id.into(),
            privacy_class: "local".into(),
            state,
            placement: None,
            profile: None,
            has_vision,
        }
    }

    #[test]
    fn canvas_uses_loaded_vision_model_only_when_chat_model_is_absent() {
        let models = vec![
            model("vision-on-disk", ModelState::OnDisk, true),
            model("text-loaded", ModelState::Loaded, false),
            model("vision-loaded", ModelState::PartiallyOffloaded, true),
        ];
        assert_eq!(
            canvas_model_id(None, &models).as_deref(),
            Some("vision-loaded")
        );
        assert_eq!(
            canvas_model_id(Some("chosen-text".into()), &models).as_deref(),
            Some("chosen-text")
        );
    }

    #[test]
    fn create_module_dump_delegates_instead_of_display() {
        let dumped = r#"{"kind":"column","children":[{"kind":"heading","text":"Ping"}]}"#;
        let spec = chat_delegate_agent_spec("crée un module ping", dumped, false, ASPECT, &full_canvas_exported());
        let (brief, _skills, tools, prose) = spec.expect("doit déléguer");
        assert_eq!(brief, "crée un module ping");
        assert!(tools.iter().any(|x| x == "module.scaffold"));
        assert!(prose.contains("agent"));
    }

    #[test]
    fn explain_module_does_not_delegate() {
        assert!(chat_delegate_agent_spec(
            "c'est quoi un module",
            "Un module est un package.",
            false,
            ASPECT,
            &full_canvas_exported(),
        )
        .is_none());
    }

    #[test]
    fn model_scaffold_action_delegates() {
        let out = r#"{"action":"module.scaffold","args":{"name":"ping"}}"#;
        let spec = chat_delegate_agent_spec("fais un ping", out, false, ASPECT, &full_canvas_exported());
        let (_brief, _skills, tools, _) = spec.expect("doit déléguer");
        assert!(tools.iter().any(|x| x == "module.scaffold"));
    }

    #[test]
    fn tts_ask_does_not_delegate_agent() {
        let out = r#"{"action":"agent.spawn","args":{"brief":"tts"}}"#;
        assert!(chat_delegate_agent_spec("génère un audio qui dit bonjour", out, false, ASPECT, &full_canvas_exported()).is_none());
        let (_skills, tools) = chat_agent_kit("génère un audio de bonjour");
        assert!(tools.iter().any(|t| t == "media.audio.generate"));
    }

    #[test]
    fn draw_request_delegates_with_image_tools_when_canvas_closed() {
        let spec = chat_delegate_agent_spec("dessine une maison", "Ok.", false, ASPECT, &full_canvas_exported());
        let (_brief, _skills, tools, _prose) = spec.expect("doit déléguer image");
        assert!(tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "canvas.stroke"));
    }

    #[test]
    fn draw_request_delegates_with_canvas_tools_when_canvas_open() {
        let spec = chat_delegate_agent_spec("dessine une maison", "Ok.", true, ASPECT, &full_canvas_exported())
            .expect("canvas ouvert + dessine doit déléguer canvas");
        let (_brief, _skills, tools, _prose) = spec;
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "user.ask"));
        assert!(!tools.iter().any(|x| x == "agent.spawn"));
        assert!(!tools.iter().any(|x| x == "agent.await"));
        assert!(!tools.iter().any(|x| x == "canvas.fill"));
    }

    #[test]
    fn explicit_canvas_delegates_with_canvas_tools() {
        let spec = chat_delegate_agent_spec("dessine sur le canvas une maison", "Ok.", false, ASPECT, &full_canvas_exported());
        let (brief, skills, tools, prose) = spec.expect("doit déléguer canvas");
        assert_eq!(brief, "dessine sur le canvas une maison");
        assert!(!brief.contains("toit + murs"));
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "user.ask"));
        assert!(!tools.iter().any(|x| x == "agent.spawn"));
        assert!(!skills.iter().any(|s| s == "planner"));
        assert!(prose.to_lowercase().contains("canvas") || prose.contains("dessin"));
    }

    #[test]
    fn canvas_delegate_brief_is_user_goal_not_designer_guide() {
        let spec = chat_delegate_agent_spec(
            "dessine une canette Coca-Cola sur le canvas",
            "Ok.",
            false,
            ASPECT,
            &full_canvas_exported(),
        )
        .expect("canvas delegate");
        let (brief, _skills, tools, _) = spec;
        assert!(tools.iter().any(|x| x.starts_with("canvas.")));
        assert_eq!(brief, "dessine une canette Coca-Cola sur le canvas");
        assert!(!brief.contains("Exemple si le sujet est une maison"));
        assert!(!brief.contains("canvas.set_style"));
    }

    #[test]
    fn dans_le_canvas_delegates_with_canvas_tools() {
        let spec = chat_delegate_agent_spec("dessine dans le canvas", "Ok.", false, ASPECT, &full_canvas_exported())
            .expect("dessine dans le canvas doit déléguer canvas");
        let (_brief, _skills, tools, _prose) = spec;
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "user.ask"));
    }

    #[test]
    fn bare_dessine_delegates_with_image_tools() {
        let spec = chat_delegate_agent_spec("dessine une maison", "Ok.", false, ASPECT, &full_canvas_exported())
            .expect("dessine une maison doit déléguer image");
        let (_brief, _skills, tools, _prose) = spec;
        assert!(tools.iter().any(|x| x == "media.image.generate"));
        assert!(!tools.iter().any(|x| x == "canvas.stroke"));
    }

    #[test]
    fn canvas_followup_when_open_does_not_delegate() {
        assert!(chat_delegate_agent_spec(
            "essai encore en ajoutant plus de détails",
            "D'accord.",
            true,
            ASPECT,
            &full_canvas_exported(),
        )
        .is_none());
        assert!(chat_delegate_agent_spec("vas y", "Ok.", true, ASPECT, &full_canvas_exported()).is_none());
    }

    #[test]
    fn canvas_truncated_spawn_explicit_canvas_delegates() {
        let out = r#"{"action":"agent.spawn","args":{"brief":"Génération d'une maison médiévale avec plus de détails en cours..."#;
        let spec = chat_delegate_agent_spec("dessine sur le canvas", out, false, ASPECT, &full_canvas_exported());
        let (_brief, _skills, tools, _) = spec.expect("JSON tronqué + explicit canvas");
        assert!(tools.iter().any(|x| x == "canvas.stroke"));
    }

    #[test]
    fn canvas_truncated_spawn_followup_does_not_delegate() {
        let out = r#"{"action":"agent.spawn","args":{"brief":"Génération..."#;
        assert!(chat_delegate_agent_spec("vas y", out, true, ASPECT, &full_canvas_exported()).is_none());
    }

    #[test]
    fn explicit_canvas_after_image_delegate_gets_canvas_tools() {
        let image = chat_delegate_agent_spec("dessine une maison", "Ok.", false, ASPECT, &full_canvas_exported())
            .expect("image delegate");
        let image_tools = image.2;
        assert!(image_tools.iter().any(|x| x == "media.image.generate"));
        assert!(!image_tools.iter().any(|x| x == "canvas.stroke"));

        let canvas = chat_delegate_agent_spec("dessine sur le canvas", "Ok.", false, ASPECT, &full_canvas_exported())
            .expect("canvas delegate after image");
        let canvas_tools = canvas.2;
        assert!(canvas_tools.iter().any(|x| x == "canvas.stroke"));
        assert!(!canvas_tools.iter().any(|x| x == "media.image.generate"));
    }

    #[test]
    fn prompts_never_mention_canvas_draw() {
        let brief = chat_canvas::canvas_agent_brief(
            "dessine sur le canvas",
            ASPECT,
            &full_canvas_exported(),
        );
        assert!(!brief.contains("canvas.draw"));
        assert!(brief.contains("canvas.stroke"));
        assert!(brief.contains("carré 1:1"));
        assert!(brief.contains("jamais canvas.clear"));
        assert!(!brief.contains("canvas.*"));
        assert!(!aos_proto::CHAT_DELEGATION_PROMPT.contains("canvas.draw"));
    }

    #[test]
    fn canvas_followup_without_open_does_not_delegate() {
        assert!(chat_delegate_agent_spec(
            "essai encore en ajoutant plus de détails",
            "D'accord.",
            false,
            ASPECT,
            &full_canvas_exported(),
        )
        .is_none());
    }

    #[test]
    fn delegate_kit_includes_canvas_path_when_module_exports_it() {
        let exported = vec![
            "canvas.path".into(),
            "canvas.stroke".into(),
            "canvas.get".into(),
        ];
        let (_, tools) = chat_delegate_kit("dessine un moulin", true, true, &exported);
        assert!(tools.iter().any(|t| t == "canvas.path"));
        assert!(tools.iter().any(|t| t == "plan.update"));
        assert!(!tools.iter().any(|t| t.starts_with("notes.")));
        assert!(!tools.iter().any(|t| t.starts_with("tasks.")));
        let brief = chat_canvas::canvas_agent_brief("dessine un moulin", ASPECT, &exported);
        assert!(brief.contains("canvas.path"));
        assert!(brief.contains("pièce manquante"));
    }

    #[test]
    fn delegate_kit_omits_canvas_path_when_module_does_not_export_it() {
        let exported = vec![
            "canvas.stroke".into(),
            "canvas.spline".into(),
            "canvas.rect".into(),
            "canvas.get".into(),
        ];
        let (_, tools) = chat_delegate_kit("dessine un moulin", true, true, &exported);
        assert!(!tools.iter().any(|t| t == "canvas.path"));
        let brief = chat_canvas::canvas_agent_brief("dessine un moulin", ASPECT, &exported);
        assert!(!brief.contains("canvas.path"));
    }
}

#[cfg(test)]
mod research_document_tests {
    use aos_agent::document_prep::{compose_document, BrowsePage};
    use aos_proto::WebSearchHit;

    #[test]
    fn user_requested_document_skips_choice_card() {
        assert!(aos_agent::research_detect::user_requested_document(
            "Please prepare a document about agentic apps"
        ));
    }

    #[test]
    fn research_shaped_ask_triggers_choice_path() {
        assert!(aos_agent::research_detect::is_research_shaped_ask(
            "what is the state of the art of agentic apps?"
        ));
    }

    #[test]
    fn answer_choice_does_not_require_document_footnotes() {
        assert!(!crate::research_choice::choice_actions_enabled("answer"));
    }

    #[test]
    fn prepare_document_mock_has_verified_footnote() {
        let hits = vec![WebSearchHit {
            title: "Agentic apps overview".into(),
            url: "https://example.org/agentic".into(),
            snippet: "Tool-using agents are maturing.".into(),
        }];
        let pages = vec![BrowsePage {
            url: "https://example.org/agentic".into(),
            title: "Agentic apps overview".into(),
            text: "Full page body.".into(),
            fetch_error: None,
        }];
        let md = compose_document("Agentic apps?", &hits, &pages);
        assert!(md.contains("[^1]: Agentic apps overview — https://example.org/agentic"));
        assert!(!md.contains("https://invented.example"));
    }
}

#[cfg(test)]
mod canvas_completion_tests {
    use super::*;
    use aos_proto::{AgentInfo, AgentKind, AgentState, AgentTrace};

    fn canvas_agent(agent_id: &str) -> AgentInfo {
        AgentInfo {
            agent_id: agent_id.into(),
            state: AgentState::Failed,
            directive: "dessine un moulin".into(),
            pid: None,
            caps: vec![],
            last_output: String::new(),
            step: 64,
            max_steps: 64,
            current_task: None,
            parent_id: None,
            children: vec![],
            tokens_used: 0,
            skills: vec![],
            tools: vec!["canvas.stroke".into()],
            mcp_servers: vec![],
            fail_reason: Some("max_steps (64) atteint".into()),
            session_id: Some("sess-1".into()),
            title: String::new(),
            kind: AgentKind::Task,
            display_name: None,
            persona_id: None,
            origin: None,
        }
    }

    #[test]
    fn completion_chat_muted_when_canvas_has_traits() {
        let t = i18n::strings("fr");
        let ag = canvas_agent("agent-102");
        let ops = vec![aos_proto::CanvasOp {
            seq: 1,
            author_id: "agent-102".into(),
            ts_ms: 0,
            layer_id: String::new(),
            body: aos_proto::CanvasOpBody::Stroke {
                points: vec![aos_proto::CanvasPoint { x: 0.1, y: 0.2 }],
                color: "#3ee0c4".into(),
                width: 0.01,
                opacity: 1.0,
                dash: vec![],
            },
        }];
        let text = agent_completion_chat_text(&ag, &t, Some(&ops), None, false, 0);
        assert!(text.is_empty(), "expected mute, got: {text}");
        assert!(!text.contains("agent-102"));
        assert!(!text.contains("terminé"));
    }

    #[test]
    fn completion_chat_note_agent_success_mutes_english_body() {
        let t = i18n::strings("fr");
        let mut ag = canvas_agent("note-agent");
        ag.state = AgentState::Done;
        ag.tools = vec!["notes.create".into()];
        ag.skills = vec!["notes-writer".into()];
        ag.last_output = "Note created successfully with title cohort.".into();
        let text = agent_completion_chat_text(&ag, &t, None, None, true, 1);
        assert!(text.is_empty(), "expected mute, got: {text}");
        assert!(!text.to_ascii_lowercase().contains("created"));
        assert!(!text.contains("cohort"));
    }

    #[test]
    fn completion_chat_note_agent_empty_list_shows_locked_fail_copy() {
        let t = i18n::strings("en");
        let mut ag = canvas_agent("note-agent");
        ag.state = AgentState::Done;
        ag.tools = vec!["notes.create".into()];
        ag.skills = vec!["notes-writer".into()];
        ag.last_output = "Note created successfully with title cohort.".into();
        let text = agent_completion_chat_text(&ag, &t, None, None, true, 0);
        assert_eq!(text, t.notes_create_failed);
        assert!(!text.to_ascii_lowercase().contains("created"));
        assert!(!text.contains("cohort"));
    }

    #[test]
    fn completion_chat_empty_canvas_shows_locked_fail_copy() {
        let t = i18n::strings("fr");
        let ag = canvas_agent("agent-90");
        let text = agent_completion_chat_text(&ag, &t, None, None, false, 0);
        assert_eq!(text, t.canvas_draw_failed);
        assert!(!text.contains("max_steps"));
        assert!(!text.contains("agent-90"));
    }

    #[test]
    fn completion_chat_muted_from_trace_traits_without_session_ops() {
        let t = i18n::strings("en");
        let ag = canvas_agent("agent-99");
        let trace = AgentTrace {
            agent_id: "agent-99".into(),
            steps: vec![aos_proto::AgentStepRecord {
                step: 1,
                action: "canvas.spline".into(),
                tool_result: "ok seq=1".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = agent_completion_chat_text(&ag, &t, None, Some(&trace), false, 0);
        assert!(text.is_empty());
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::chat_bubble::ChatBubbleKind;
    use crate::composer_layout::{
        bounded_chat_workspace_width, chat_canvas_layout, chat_composer_reserve_height, chat_sessions_split,
        composer_field_width, ChatCanvasLayout,
    };

    #[test]
    fn chat_sessions_split_never_exceeds_total() {
        for full_w in [250.0, 400.0, 900.0, 1280.0] {
            let gap = 8.0;
            let split = chat_sessions_split(full_w, gap, false);
            assert!(
                split.side_w + gap + split.chat_w <= full_w + 0.01,
                "full_w={full_w} side={} chat={}",
                split.side_w,
                split.chat_w
            );
        }
    }

    #[test]
    fn chat_workspace_is_bounded_after_wide_session_widget() {
        // A long model id can make egui give the Sessions rail more than its
        // planned split; the right pane must shrink rather than clip its canvas.
        assert_eq!(bounded_chat_workspace_width(600.0, 520.0, 8.0), 512.0);
        assert_eq!(bounded_chat_workspace_width(600.0, 900.0, 8.0), 592.0);
        assert_eq!(bounded_chat_workspace_width(100.0, 4.0, 8.0), 0.0);
    }

    #[test]
    fn chat_canvas_side_by_side_widths_fit() {
        let gap = 8.0;
        let layout = chat_canvas_layout(500.0, 400.0, gap);
        match layout {
            ChatCanvasLayout::SideBySide {
                transcript_w,
                canvas_w,
            } => {
                assert!(transcript_w + gap + canvas_w <= 500.0 + 0.01);
            }
            ChatCanvasLayout::Stacked { .. } => panic!("expected side-by-side at 500px"),
        }
    }

    #[test]
    fn session_toggle_reserve_fits_fr_canvas_label() {
        let fr = i18n::strings("fr");
        let w = session_toggle_reserve_width(&fr);
        assert!(
            w >= estimate_label_chip_w(fr.session_toggle_canvas) + 40.0,
            "reserve {w} should fit full Canvas label"
        );
    }

    #[test]
    fn chat_canvas_stacks_before_side_by_side_threshold() {
        let gap = 8.0;
        let layout = chat_canvas_layout(300.0, 400.0, gap);
        assert!(matches!(layout, ChatCanvasLayout::Stacked { .. }));
    }

    #[test]
    fn composer_field_width_reserves_fr_envoyer() {
        let fr = i18n::strings("fr");
        let send_w = estimate_composer_buttons_w(fr.agent_send, false, "");
        let field = composer_field_width(420.0, send_w, icons::ATTACH_BTN_W, 0.0, 4.0, false);
        assert!(field > 80.0);
        assert!(send_w >= estimate_composer_buttons_w("Envoyer", false, ""));
    }

    #[test]
    fn composer_field_width_at_900_central_pane() {
        let fr = i18n::strings("fr");
        let send_w = estimate_composer_buttons_w(fr.agent_send, false, "");
        let stop_w = estimate_composer_buttons_w("Stop", false, "");
        let field =
            composer_field_width(580.0, send_w, icons::ATTACH_BTN_W, stop_w, 4.0, true);
        assert!(field > 200.0);
        assert!(
            field + send_w + stop_w + icons::ATTACH_BTN_W + 12.0 <= 580.0 + 0.01
        );
    }

    #[test]
    fn composer_reserve_height_is_single_row() {
        let h = chat_composer_reserve_height(400.0, 0, 0, 0, false);
        assert!((h - COMPOSER_INPUT_ROW_H).abs() < 0.01);
    }

    #[test]
    fn composer_row_reserved_fits_fr_labels() {
        let fr = i18n::strings("fr");
        let reserved = composer_row_reserved_width(&fr, true);
        assert!(reserved > icons::ATTACH_BTN_W + 80.0);
    }

    #[test]
    fn composer_wraps_when_too_narrow() {
        let buttons = estimate_composer_buttons_w("Envoyer", true, "Stop");
        assert!(chat_composer_wraps(280.0, icons::ATTACH_BTN_W, buttons));
        assert!(!chat_composer_wraps(900.0, icons::ATTACH_BTN_W, buttons));
    }

    #[test]
    fn preview_min_width_fits_fr_composer_and_toggles() {
        let fr = i18n::strings("fr");
        let min_w = preview_min_inner_width(&fr);
        let composer = composer_row_reserved_width(&fr, true) + COMPOSER_MIN_INPUT_W;
        assert!(min_w >= LEFT_NAV_W + CHAT_SIDE_MIN_W + composer);
        assert!(min_w >= LEFT_NAV_W + session_toggle_reserve_width(&fr) + 200.0);
    }

    #[test]
    fn bubble_max_width_never_exceeds_available() {
        for w in [80.0, 150.0, 400.0] {
            for kind in [
                ChatBubbleKind::User,
                ChatBubbleKind::Assistant,
                ChatBubbleKind::System,
            ] {
                let max_w = chat_bubble_max_width(w, kind);
                assert!(max_w <= w + 0.01, "kind={kind:?} avail={w} max={max_w}");
            }
        }
    }
}
