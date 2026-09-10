//! LLM prompt enrichment for Image Studio — Ideogram 4 vs generic JSON schemas.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptEnrichmentKind {
    /// Ideogram 4 native caption schema (high_level_description, compositional_deconstruction, …).
    Ideogram4,
    /// Generic diffusion JSON (subject, environment, style, lighting, camera, …).
    GenericJson,
}

/// System prompt for Ideogram 4 structured captions.
/// Schema: https://github.com/ideogram-oss/ideogram4/blob/main/docs/prompting.md
pub const IDEOGRAM4_SYSTEM_PROMPT: &str = r##"You convert a short user idea into a structured JSON caption for Ideogram 4. Output ONE minified single-line JSON object and NOTHING else (no markdown, no commentary).
SCHEMA — keys in this exact order:
{"high_level_description":"...","style_description":{"aesthetics":"...","lighting":"...","photo":"...","medium":"...","color_palette":["#RRGGBB", "..."]},"compositional_deconstruction":{"background":"...","elements":[ ... ]}}
- `compositional_deconstruction` is required and must contain `background` then `elements` (an array).
- Spell the key exactly: compositional_deconstruction (NOT compositional_destruction).
- `background` is a concrete scene/environment description deduced from `high_level_description` and the user's idea; do not invent a setting absent from the user idea.
- `elements` are ordered back to front. Every element is either {"type":"obj","bbox":[y_min,x_min,y_max,x_max],"desc":"..."} or {"type":"text","bbox":[y_min,x_min,y_max,x_max],"text":"VERBATIM","desc":"..."}; bbox is optional and coordinates are normalized integers 0–1000 in [y_min,x_min,y_max,x_max] order.
- `style_description` is optional, but when present it must contain `aesthetics`, `lighting`, and `medium`, plus exactly one of `photo` or `art_style` (never both). For a photo use `photo` before `medium`; for an art style use `medium` before `art_style`.
- Optional `color_palette` values must be uppercase #RRGGBB strings. Keep the documented key order: aesthetics, lighting, photo, medium, color_palette (photo) or aesthetics, lighting, medium, art_style, color_palette (art).
Rules:
1. Preserve the user's core subject and any quoted text verbatim.
2. Be specific and concrete — no vague mood-word spam.
3. Do not add elements the user did not imply.
4. Output STRICTLY VALID JSON: double quotes, NO trailing commas.
5. Emit exactly one compositional_deconstruction object — never a second composition key under another name.
6. Keep all fields as strings, arrays, or objects from this schema; do not emit markdown or a prose wrapper."##;

const IDEOGRAM_DECONSTRUCTION_ALIASES: &[&str] = &[
    "compositional_destruction",
    "compositional_deconstrution",
    "composition_deconstruction",
];

fn string_value(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn normalized_palette(value: Option<&serde_json::Value>, max: usize) -> Option<serde_json::Value> {
    let values = value?.as_array()?;
    let colors: Vec<String> = values
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| {
            s.len() == 7
                && s.as_bytes()[0] == b'#'
                && s.as_bytes()[1..].iter().all(|b| b.is_ascii_hexdigit())
        })
        .take(max)
        .map(|s| s.to_ascii_uppercase())
        .collect();
    (!colors.is_empty()).then(|| serde_json::json!(colors))
}

fn normalized_bbox(value: Option<&serde_json::Value>) -> Option<serde_json::Value> {
    let values = value?.as_array()?;
    if values.len() != 4 {
        return None;
    }
    let coords: Option<Vec<i64>> = values
        .iter()
        .map(|v| v.as_i64().or_else(|| v.as_f64().map(|n| n.round() as i64)))
        .collect();
    let coords = coords?;
    if coords.iter().any(|n| !(0..=1000).contains(n)) {
        return None;
    }
    Some(serde_json::json!(coords))
}

fn normalized_element(value: &serde_json::Value) -> Option<serde_json::Value> {
    let source = value.as_object()?;
    let kind = source
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("obj")
        .to_ascii_lowercase();
    let desc = string_value(source.get("desc").or_else(|| source.get("description")))?;
    let bbox = normalized_bbox(source.get("bbox"));
    let mut out = serde_json::Map::new();
    if kind == "text" {
        let text = source
            .get("text")
            .and_then(serde_json::Value::as_str)?
            .to_owned();
        out.insert("type".into(), serde_json::json!("text"));
        if let Some(bbox) = bbox {
            out.insert("bbox".into(), bbox);
        }
        out.insert("text".into(), serde_json::json!(text));
        out.insert("desc".into(), serde_json::json!(desc));
    } else if kind == "obj" || kind == "object" {
        out.insert("type".into(), serde_json::json!("obj"));
        if let Some(bbox) = bbox {
            out.insert("bbox".into(), bbox);
        }
        out.insert("desc".into(), serde_json::json!(desc));
    } else {
        return None;
    }
    if let Some(palette) = normalized_palette(source.get("color_palette"), 5) {
        out.insert("color_palette".into(), palette);
    }
    Some(serde_json::Value::Object(out))
}

fn meaningful_background(value: Option<&serde_json::Value>) -> Option<String> {
    let text = string_value(value)?;
    let lower = text.to_ascii_lowercase();
    (!lower.contains("unspecified") && !lower.eq_ignore_ascii_case("background")).then_some(text)
}

fn derive_background(description: &str) -> String {
    let description = description.trim();
    // Keep a location clause when the caption contains a common spatial
    // preposition; otherwise retain the complete high-level description so
    // the background remains grounded in the user's prompt rather than being
    // replaced by a fabricated scene.
    let lower = description.to_ascii_lowercase();
    let markers = [
        " in ",
        " at ",
        " on ",
        " under ",
        " inside ",
        " near ",
        " against ",
        " dans ",
        " au ",
        " aux ",
        " en ",
        " sur ",
        " sous ",
        " devant ",
        " à ",
    ];
    markers
        .iter()
        .filter_map(|marker| lower.find(marker).map(|index| (index, marker.len())))
        .min_by_key(|(index, _)| *index)
        .map(|(index, marker_len)| description[index + marker_len..].trim())
        .filter(|tail| !tail.is_empty())
        .map(|tail| tail.trim_end_matches(['.', ';']).to_owned())
        .filter(|tail| tail.len() >= 3)
        .unwrap_or_else(|| description.to_owned())
}

fn json_fragment(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

fn ordered_style_json(style: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut fields = Vec::with_capacity(5);
    for key in ["aesthetics", "lighting"] {
        if let Some(value) = style.get(key) {
            fields.push((format!("\"{key}\""), value));
        }
    }
    if let Some(value) = style.get("photo") {
        fields.push(("\"photo\"".into(), value));
        if let Some(medium) = style.get("medium") {
            fields.push(("\"medium\"".into(), medium));
        }
    } else {
        if let Some(medium) = style.get("medium") {
            fields.push(("\"medium\"".into(), medium));
        }
        if let Some(art_style) = style.get("art_style") {
            fields.push(("\"art_style\"".into(), art_style));
        }
    }
    if let Some(palette) = style.get("color_palette") {
        fields.push(("\"color_palette\"".into(), palette));
    }
    let fragments = fields
        .into_iter()
        .map(|(key, value)| format!("{key}:{}", json_fragment(value)))
        .collect::<Vec<_>>();
    format!("{{{}}}", fragments.join(","))
}

fn ordered_element_json(element: &serde_json::Value) -> String {
    let Some(object) = element.as_object() else {
        return json_fragment(element);
    };
    let mut fields = Vec::with_capacity(5);
    for key in ["type", "bbox", "text", "desc", "color_palette"] {
        if let Some(value) = object.get(key) {
            fields.push(format!("\"{key}\":{}", json_fragment(value)));
        }
    }
    format!("{{{}}}", fields.join(","))
}

/// Validate and normalize a model response to the official Ideogram 4 caption shape.
///
/// LLMs occasionally use the historical `compositional_destruction` typo, emit a
/// generic diffusion JSON object, or include both `photo` and `art_style`. Repairing
/// those cases here keeps the payload sent to Ideogram deterministic and also makes
/// the exact prompt visible in Create's UI/history.
pub fn normalize_ideogram_caption(raw: &str, fallback: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(raw.trim())
        .map_err(|err| format!("Ideogram caption is not valid JSON: {err}"))?;
    let root = value
        .as_object()
        .ok_or_else(|| "Ideogram caption must be a JSON object".to_string())?;
    let high_level = string_value(root.get("high_level_description"))
        .or_else(|| string_value(root.get("subject")))
        .or_else(|| string_value(root.get("description")))
        .or_else(|| (!fallback.trim().is_empty()).then(|| fallback.trim().to_owned()))
        .unwrap_or_default();

    let style_source = root
        .get("style_description")
        .and_then(serde_json::Value::as_object);
    let style_hint = string_value(root.get("style"))
        .or_else(|| string_value(root.get("medium")))
        .unwrap_or_default();
    let explicit_photo = style_source.and_then(|s| string_value(s.get("photo")));
    let explicit_art = style_source.and_then(|s| string_value(s.get("art_style")));
    let style_hint_lower = style_hint.to_ascii_lowercase();
    let is_photo = explicit_photo.is_some()
        || (explicit_art.is_none()
            && ["photo", "photograph", "photoreal", "realistic"]
                .iter()
                .any(|hint| style_hint_lower.contains(hint)));
    let aesthetics = style_source
        .and_then(|s| string_value(s.get("aesthetics")))
        .or_else(|| string_value(root.get("aesthetics")))
        .unwrap_or_else(|| "natural, detailed".into());
    let lighting = style_source
        .and_then(|s| string_value(s.get("lighting")))
        .or_else(|| string_value(root.get("lighting")))
        .unwrap_or_else(|| "soft natural light".into());
    let medium = style_source
        .and_then(|s| string_value(s.get("medium")))
        .unwrap_or_else(|| {
            if is_photo {
                "photograph"
            } else {
                "digital illustration"
            }
            .into()
        });
    let mut style = serde_json::Map::new();
    style.insert("aesthetics".into(), serde_json::json!(aesthetics));
    style.insert("lighting".into(), serde_json::json!(lighting));
    if is_photo {
        style.insert(
            "photo".into(),
            serde_json::json!(explicit_photo.unwrap_or_else(|| "photograph".into())),
        );
        style.insert("medium".into(), serde_json::json!(medium));
    } else {
        style.insert("medium".into(), serde_json::json!(medium));
        style.insert(
            "art_style".into(),
            serde_json::json!(explicit_art.unwrap_or_else(|| {
                if style_hint.trim().is_empty() {
                    "digital illustration".into()
                } else {
                    style_hint.clone()
                }
            })),
        );
    }
    let palette = style_source
        .and_then(|s| normalized_palette(s.get("color_palette"), 16))
        .or_else(|| normalized_palette(root.get("color_palette"), 16));
    if let Some(palette) = palette {
        style.insert("color_palette".into(), palette);
    }

    let composition = root.get("compositional_deconstruction").or_else(|| {
        IDEOGRAM_DECONSTRUCTION_ALIASES
            .iter()
            .find_map(|k| root.get(*k))
    });
    let background = composition
        .and_then(serde_json::Value::as_object)
        .and_then(|c| meaningful_background(c.get("background")))
        .or_else(|| meaningful_background(root.get("background")))
        .or_else(|| meaningful_background(root.get("environment")))
        .or_else(|| (!high_level.is_empty()).then(|| derive_background(&high_level)))
        .unwrap_or_else(|| "unspecified background".into());
    let source_elements = composition
        .and_then(serde_json::Value::as_object)
        .and_then(|c| c.get("elements"))
        .or_else(|| root.get("elements"));
    let mut elements: Vec<serde_json::Value> = source_elements
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().filter_map(normalized_element).collect())
        .unwrap_or_default();
    if elements.is_empty() && !high_level.is_empty() {
        elements.push(serde_json::json!({"type":"obj","desc":high_level}));
    }

    let mut composition_out = serde_json::Map::new();
    composition_out.insert("background".into(), serde_json::json!(background));
    composition_out.insert("elements".into(), serde_json::json!(elements));
    let background_json = json_fragment(composition_out.get("background").expect("background"));
    let mut out = serde_json::Map::new();
    out.insert(
        "high_level_description".into(),
        serde_json::json!(high_level),
    );
    out.insert("style_description".into(), serde_json::Value::Object(style));
    out.insert(
        "compositional_deconstruction".into(),
        serde_json::Value::Object(composition_out),
    );
    let elements_json = elements
        .iter()
        .map(ordered_element_json)
        .collect::<Vec<_>>()
        .join(",");
    let composition_json = format!(
        "{{\"background\":{},\"elements\":[{}]}}",
        background_json, elements_json
    );
    let high_json = json_fragment(out.get("high_level_description").expect("description"));
    let style_json = ordered_style_json(out["style_description"].as_object().expect("style"));
    Ok(format!(
        "{{\"high_level_description\":{high_json},\"style_description\":{style_json},\"compositional_deconstruction\":{composition_json}}}"
    ))
}

/// Generic JSON schema for Flux, SDXL, Z-Image, Qwen Image, Krea, Wan, etc.
/// Refs: ImagineArt JSON prompting guide, diffusion JSON prompting articles.
pub const GENERIC_JSON_SYSTEM_PROMPT: &str = r##"You convert a short user idea into a structured JSON prompt for AI image generation. Output ONE minified single-line JSON object and NOTHING else (no markdown, no commentary).
Use this schema (omit keys that do not apply; keep values concrete):
{"subject":"...","action":"...","environment":"...","background":"...","style":"...","lighting":"...","camera":{"lens":"...","aperture":"...","angle":"...","depth_of_field":"..."},"mood":"...","time_of_day":"...","weather":"...","colors":"...","composition":"...","details":"...","text_in_image":"..."}
Rules:
1. Preserve the user's core subject and any quoted text verbatim in subject/action/text_in_image.
2. Be specific: materials, colors, counts, positions — avoid "beautiful", "stunning", "vibrant".
3. Do not add elements the user did not imply.
4. Valid JSON only: double quotes, no trailing commas."##;

/// Rewrites a short user idea into a detailed prose prompt for diffusion models (SD, DiT, etc.).
pub const CHAT_ENHANCE_SYSTEM_PROMPT: &str = r##"You expand a short image idea into a rich, concrete text-to-image prompt. Output ONLY the improved prompt as plain text — no markdown, no JSON, no labels, no commentary before or after.
Include when relevant: subject, action, environment, lighting, colors, materials, camera/composition, mood, and style medium (photo, illustration, 3d, etc.).
Rules:
1. Preserve the user's core subject and any quoted text verbatim.
2. Add plausible, specific details — avoid vague filler ("beautiful", "stunning", "masterpiece").
3. Do not add major elements the user did not imply.
4. One paragraph or a few comma-separated phrases; keep under 120 words unless the user idea is already long."##;

/// Concise rewrite used for composition layers. Unlike the global assistant,
/// this must preserve the element's scope and geometry instead of inventing a
/// complete scene, camera setup, or unrelated objects.
pub const CHAT_ENHANCE_LAYER_SYSTEM_PROMPT: &str = r##"Rewrite a single composition-layer description into concise plain text. Output ONLY the rewritten description, with no markdown, JSON, labels, or commentary.
Preserve exactly the same subject, action, and intended object. Add at most two concrete visual details (material, color, or one simple state). Never invent a new setting, additional objects or people, camera/composition instructions, lighting setup, or a complete scene. Keep it under 35 words and keep the original language when possible."##;

/// Video-specific prose enrichment. Keep the output plain text because sd.cpp
/// receives it as the actual prompt, but add temporal/camera cues that an
/// image-only rewrite would otherwise omit.
pub const CHAT_ENHANCE_VIDEO_SYSTEM_PROMPT: &str = r##"You expand a short idea into a production-ready prompt for a short AI-generated video. Output ONLY the improved prompt as plain text — no markdown, no JSON, no labels, no commentary before or after.
Use a compact three-sentence structure: (1) subject, setting and one continuous physical action; (2) exactly one primary camera move plus framing, pace and concrete light/material cues; (3) a positive continuity rule that keeps the subject, composition and motion stable. Add sound only when it is implied or useful.
Rules:
1. Preserve the user's core subject and any quoted text verbatim.
2. Prefer one shot and one camera vector (for example, slow push-in or lateral tracking); never stack pan, orbit, tilt and zoom together.
3. Describe observable motion with a clear direction and speed; avoid vague quality adjectives and frantic speed words.
4. For an image reference, do not re-describe the image: specify only the added motion, camera instruction and one consistency constraint.
5. State constraints positively ("keep the silhouette stable") instead of a list of negative prompts.
6. Do not add major characters, objects, or scene changes the user did not imply. Keep under 120 words."##;

pub fn is_video_prompt_model(model_id: Option<&str>) -> bool {
    model_id.is_some_and(|id| id.contains("wan") || id.contains("ltx") || id.contains("minimax"))
}

fn is_upscale_model(id: &str) -> bool {
    id.contains("realesrgan") || id.contains("upscale")
}

fn is_image_generation_model(id: &str) -> bool {
    if id.is_empty() || is_upscale_model(id) {
        return false;
    }
    id.starts_with("local:")
        && (id.contains("sd-v1")
            || id.contains("sdxl")
            || id.contains("flux")
            || id.contains("ideogram")
            || id.contains("z-image")
            || id.contains("qwen-image")
            || id.contains("krea")
            || id.contains("wan")
            || id.contains("ltx")
            || id.contains("minimax"))
}

/// Which JSON schema applies to this image model, if any.
pub fn prompt_enrichment_kind(model_id: &str) -> Option<PromptEnrichmentKind> {
    if !is_image_generation_model(model_id) {
        return None;
    }
    if model_id.contains("ideogram") {
        Some(PromptEnrichmentKind::Ideogram4)
    } else {
        Some(PromptEnrichmentKind::GenericJson)
    }
}

/// True when Image Studio can offer LLM → JSON enrichment for this model.
pub fn supports_json_prompt_enrichment(model_id: Option<&str>) -> bool {
    model_id.and_then(prompt_enrichment_kind).is_some()
}

/// Default « enrich prompt » checkbox: on for Ideogram 4 only.
pub fn default_enrich_prompt(model_id: Option<&str>) -> bool {
    matches!(
        model_id.and_then(prompt_enrichment_kind),
        Some(PromptEnrichmentKind::Ideogram4)
    )
}

pub fn enrichment_system_prompt(kind: PromptEnrichmentKind) -> &'static str {
    match kind {
        PromptEnrichmentKind::Ideogram4 => IDEOGRAM4_SYSTEM_PROMPT,
        PromptEnrichmentKind::GenericJson => GENERIC_JSON_SYSTEM_PROMPT,
    }
}

pub fn enrichment_status_label(kind: PromptEnrichmentKind) -> &'static str {
    match kind {
        PromptEnrichmentKind::Ideogram4 => "Ideogram 4",
        PromptEnrichmentKind::GenericJson => "Image",
    }
}

/// Heavy diffusion packs that benefit from sd.cpp offload / FA defaults.
pub fn is_heavy_image_model(model_id: &str) -> bool {
    model_id.contains("ideogram")
        || model_id.contains("flux")
        || model_id.contains("z-image")
        || model_id.contains("qwen-image")
        || model_id.contains("krea")
        || model_id.contains("wan")
        || model_id.contains("ltx")
        || model_id.contains("minimax")
        || model_id.contains("sdxl")
}

#[cfg(test)]
mod tests {
    use super::{
        is_video_prompt_model, normalize_ideogram_caption, prompt_enrichment_kind,
        PromptEnrichmentKind,
    };

    #[test]
    fn video_models_use_temporal_prompt_enrichment() {
        assert!(is_video_prompt_model(Some("local:ltx2.3-dev")));
        assert!(is_video_prompt_model(Some("local:wan2.2-t2v")));
        assert!(is_video_prompt_model(Some("local:minimax-h3")));
        assert!(!is_video_prompt_model(Some("local:sd-v1-5")));
        assert!(!is_video_prompt_model(None));
    }

    #[test]
    fn z_image_keeps_generic_structured_prompt_support() {
        assert_eq!(
            prompt_enrichment_kind("local:z-image-turbo"),
            Some(PromptEnrichmentKind::GenericJson)
        );
    }

    #[test]
    fn ideogram_caption_repairs_alias_and_style_branch() {
        let raw = r###"{"high_level_description":"a cat","style_description":{"aesthetics":"cinematic","lighting":"soft","photo":"film","art_style":"watercolor","medium":"photo","color_palette":["#aa11ff","invalid"]},"compositional_destruction":{"background":"snowy street","elements":[{"type":"obj","bbox":[10,20,900,800],"desc":"cat"}]}}"###;
        let normalized = normalize_ideogram_caption(raw, "a cat").unwrap();
        assert!(normalized.starts_with("{\"high_level_description\""));
        assert!(normalized.contains("\"style_description\":{\"aesthetics\""));
        assert!(normalized.contains("\"compositional_deconstruction\":{\"background\""));
        let value: serde_json::Value = serde_json::from_str(&normalized).unwrap();
        assert!(value.get("compositional_destruction").is_none());
        assert!(value["style_description"].get("photo").is_some());
        assert!(value["style_description"].get("art_style").is_none());
        assert_eq!(value["style_description"]["color_palette"][0], "#AA11FF");
        assert_eq!(
            value["compositional_deconstruction"]["background"],
            "snowy street"
        );
    }

    #[test]
    fn ideogram_caption_builds_required_elements_from_generic_json() {
        let raw = r#"{"subject":"a red fox","environment":"pine forest","style":"watercolor","lighting":"moonlight"}"#;
        let normalized = normalize_ideogram_caption(raw, "a red fox").unwrap();
        let value: serde_json::Value = serde_json::from_str(&normalized).unwrap();
        assert_eq!(value["high_level_description"], "a red fox");
        assert_eq!(
            value["compositional_deconstruction"]["background"],
            "pine forest"
        );
        assert_eq!(
            value["compositional_deconstruction"]["elements"][0]["desc"],
            "a red fox"
        );
        assert_eq!(value["style_description"]["art_style"], "watercolor");
        assert!(value["style_description"].get("photo").is_none());
    }

    #[test]
    fn ideogram_caption_derives_background_from_high_level_description() {
        let raw = r#"{"high_level_description":"A red fox resting in a quiet pine forest at dawn","style_description":{"aesthetics":"natural","lighting":"dawn","medium":"photograph","photo":"wildlife"},"compositional_deconstruction":{"background":"unspecified background","elements":[]}}"#;
        let normalized = normalize_ideogram_caption(raw, "").unwrap();
        let value: serde_json::Value = serde_json::from_str(&normalized).unwrap();
        assert_eq!(
            value["compositional_deconstruction"]["background"],
            "a quiet pine forest at dawn"
        );
    }
}
