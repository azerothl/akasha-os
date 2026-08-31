//! LLM prompt enrichment for Image Studio — Ideogram 4 vs generic JSON schemas.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptEnrichmentKind {
    /// Ideogram 4 native caption schema (high_level_description, compositional_deconstruction, …).
    Ideogram4,
    /// Generic diffusion JSON (subject, environment, style, lighting, camera, …).
    GenericJson,
}

/// System prompt for Ideogram 4 structured captions.
/// Schema: https://huggingface.co/AgentAnon/Ideogram4_NSFW_Loras_and_Checkpoints/blob/main/ideogram%204%20prompting.md
pub const IDEOGRAM4_SYSTEM_PROMPT: &str = r##"You convert a short user idea into a structured JSON caption for Ideogram 4. Output ONE minified single-line JSON object and NOTHING else (no markdown, no commentary).
SCHEMA — keys in this exact order:
{"high_level_description":"...","style_description":{"aesthetics":"...","lighting":"...","photo":"...","medium":"...","color_palette":["#RRGGBB", "..."]},"compositional_deconstruction":{"background":"...","elements":[ ... ]}}
- Spell the key exactly: compositional_deconstruction (NOT compositional_destruction).
- compositional_deconstruction.background must be a non-empty scene/environment string when the user implies a setting.
- object element: {"type":"obj","desc":"..."} or with optional "bbox":[y_min,x_min,y_max,x_max] in 0–1000 coords
- text element: {"type":"text","text":"VERBATIM","desc":"..."}
style_description must contain exactly one of "photo" (with medium "photograph") or "art_style" (illustration, 3d_render, etc.), never both.
Key order in style_description: aesthetics, lighting, then photo OR medium+art_style, then optional color_palette (uppercase #RRGGBB hex).
Rules:
1. Preserve the user's core subject and any quoted text verbatim.
2. Be specific and concrete — no vague mood-word spam.
3. Do not add elements the user did not imply.
4. Output STRICTLY VALID JSON: double quotes, NO trailing commas.
5. Emit exactly one compositional_deconstruction object — never a second composition key under another name."##;

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
            || id.contains("ltx"))
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
    model_id
        .and_then(prompt_enrichment_kind)
        .is_some()
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
        || model_id.contains("sdxl")
}
