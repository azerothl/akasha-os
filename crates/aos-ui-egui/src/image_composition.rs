//! Prompt layout injection for `media.image.generate` (composition blocks).

use crate::image_prompt::{
    normalize_ideogram_caption, prompt_enrichment_kind, PromptEnrichmentKind,
};
use image::{GrayImage, Luma};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Cursor;
use std::path::PathBuf;

/// Placement block in normalized frame coords (0..1). Vec order = z-order (back → front).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompositionBlock {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub desc: String,
}

impl CompositionBlock {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            x: 0.35,
            y: 0.35,
            w: 0.30,
            h: 0.30,
            desc: String::new(),
        }
    }

    pub fn clamp_in_frame(&mut self) {
        self.w = self.w.clamp(0.05, 1.0);
        self.h = self.h.clamp(0.05, 1.0);
        self.x = self.x.clamp(0.0, 1.0 - self.w);
        self.y = self.y.clamp(0.0, 1.0 - self.h);
    }

    /// Ideogram bbox `[ymin, xmin, ymax, xmax]` in 0–1000.
    pub fn ideogram_bbox(&self) -> [i32; 4] {
        let xmin = (self.x * 1000.0).round() as i32;
        let ymin = (self.y * 1000.0).round() as i32;
        let xmax = ((self.x + self.w) * 1000.0).round() as i32;
        let ymax = ((self.y + self.h) * 1000.0).round() as i32;
        [
            ymin.clamp(0, 1000),
            xmin.clamp(0, 1000),
            ymax.clamp(0, 1000),
            xmax.clamp(0, 1000),
        ]
    }
}

fn active_blocks(blocks: &[CompositionBlock]) -> Vec<&CompositionBlock> {
    blocks
        .iter()
        .filter(|b| !b.desc.trim().is_empty())
        .collect()
}

fn plain_layout_transcript(blocks: &[&CompositionBlock]) -> String {
    let parts: Vec<String> = blocks
        .iter()
        .enumerate()
        .map(|(z, b)| {
            let left = (b.x * 100.0).round() as i32;
            let top = (b.y * 100.0).round() as i32;
            let w = (b.w * 100.0).round() as i32;
            let h = (b.h * 100.0).round() as i32;
            format!(
                "[z{z} left {left}%, top {top}%, {w}%×{h}%] {}",
                b.desc.trim()
            )
        })
        .collect();
    format!("Composition (back to front): {}", parts.join("; "))
}

fn ideogram_elements(blocks: &[&CompositionBlock]) -> Vec<Value> {
    blocks
        .iter()
        .map(|b| {
            json!({
                "type": "obj",
                "desc": b.desc.trim(),
                "bbox": b.ideogram_bbox(),
            })
        })
        .collect()
}

fn generic_layout_array(blocks: &[&CompositionBlock]) -> Vec<Value> {
    blocks
        .iter()
        .enumerate()
        .map(|(z, b)| {
            json!({
                "desc": b.desc.trim(),
                "x": (b.x * 1000.0).round() / 1000.0,
                "y": (b.y * 1000.0).round() / 1000.0,
                "w": (b.w * 1000.0).round() / 1000.0,
                "h": (b.h * 1000.0).round() / 1000.0,
                "z": z,
            })
        })
        .collect()
}

fn minify_json(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "{}".into())
}

/// LLM sometimes emits this typo instead of `compositional_deconstruction`.
const IDEOGRAM_DECONSTRUCTION_ALIASES: &[&str] = &[
    "compositional_destruction",
    "compositional_deconstrution",
    "composition_deconstruction",
];

fn take_ideogram_background(obj: &serde_json::Map<String, Value>) -> Value {
    if let Some(bg) = obj
        .get("compositional_deconstruction")
        .and_then(|c| c.get("background"))
    {
        if !bg.as_str().map(|s| s.trim().is_empty()).unwrap_or(true) {
            return bg.clone();
        }
        // Keep empty only if no richer alias exists below.
        let empty = bg.clone();
        for alias in IDEOGRAM_DECONSTRUCTION_ALIASES {
            if let Some(bg2) = obj.get(*alias).and_then(|c| c.get("background")) {
                if !bg2.as_str().map(|s| s.trim().is_empty()).unwrap_or(true) {
                    return bg2.clone();
                }
            }
        }
        return empty;
    }
    for alias in IDEOGRAM_DECONSTRUCTION_ALIASES {
        if let Some(bg) = obj.get(*alias).and_then(|c| c.get("background")) {
            return bg.clone();
        }
    }
    json!("")
}

fn strip_ideogram_deconstruction_aliases(obj: &mut serde_json::Map<String, Value>) {
    for alias in IDEOGRAM_DECONSTRUCTION_ALIASES {
        obj.remove(*alias);
    }
}

/// Build a full prompt from the global text + layout blocks (no prior LLM pass).
pub fn compose_prompt_with_layout(
    base: &str,
    blocks: &[CompositionBlock],
    model_id: Option<&str>,
) -> String {
    let active = active_blocks(blocks);
    if active.is_empty() {
        return base.to_string();
    }
    let base = base.trim();
    match model_id.and_then(prompt_enrichment_kind) {
        Some(PromptEnrichmentKind::Ideogram4) => {
            let v = json!({
                "high_level_description": base,
                "compositional_deconstruction": {
                    "background": "",
                    "elements": ideogram_elements(&active),
                }
            });
            minify_json(&v)
        }
        Some(PromptEnrichmentKind::GenericJson) => {
            let v = json!({
                "subject": base,
                "composition": plain_layout_transcript(&active),
                "layout": generic_layout_array(&active),
            });
            minify_json(&v)
        }
        None => {
            if base.is_empty() {
                plain_layout_transcript(&active)
            } else {
                format!("{base}\n{}", plain_layout_transcript(&active))
            }
        }
    }
}

/// Merge layout blocks into an already-enriched prompt (JSON or prose).
pub fn merge_layout_into_prompt(
    prompt: &str,
    blocks: &[CompositionBlock],
    model_id: Option<&str>,
) -> String {
    let active = active_blocks(blocks);
    if active.is_empty() {
        return prompt.to_string();
    }
    let trimmed = prompt.trim();
    if let Ok(mut v) = serde_json::from_str::<Value>(trimmed) {
        if let Some(obj) = v.as_object_mut() {
            let has_ideogram_comp = obj.contains_key("compositional_deconstruction")
                || IDEOGRAM_DECONSTRUCTION_ALIASES
                    .iter()
                    .any(|k| obj.contains_key(*k));
            // Ideogram-style
            if has_ideogram_comp
                || matches!(
                    model_id.and_then(prompt_enrichment_kind),
                    Some(PromptEnrichmentKind::Ideogram4)
                )
            {
                let background = take_ideogram_background(obj);
                strip_ideogram_deconstruction_aliases(obj);
                obj.insert(
                    "compositional_deconstruction".into(),
                    json!({
                        "background": background,
                        "elements": ideogram_elements(&active),
                    }),
                );
                if !obj.contains_key("high_level_description") {
                    obj.insert("high_level_description".into(), json!(""));
                }
                return minify_json(&v);
            }
            // Generic JSON
            obj.insert("layout".into(), json!(generic_layout_array(&active)));
            obj.insert(
                "composition".into(),
                json!(plain_layout_transcript(&active)),
            );
            return minify_json(&v);
        }
    }
    // Prose / non-JSON
    format!("{trimmed}\n{}", plain_layout_transcript(&active))
}

/// Apply layout after optional LLM enrich / edited prompt.
pub fn finalize_prompt_with_layout(
    base_or_enriched: &str,
    blocks: &[CompositionBlock],
    model_id: Option<&str>,
    had_prior_enrichment: bool,
) -> String {
    let active = active_blocks(blocks);
    if active.is_empty() {
        return base_or_enriched.to_string();
    }
    let merged =
        if had_prior_enrichment || serde_json::from_str::<Value>(base_or_enriched.trim()).is_ok() {
            merge_layout_into_prompt(base_or_enriched, blocks, model_id)
        } else {
            compose_prompt_with_layout(base_or_enriched, blocks, model_id)
        };
    if matches!(
        model_id.and_then(prompt_enrichment_kind),
        Some(PromptEnrichmentKind::Ideogram4)
    ) {
        normalize_ideogram_caption(&merged, base_or_enriched).unwrap_or(merged)
    } else {
        merged
    }
}

/// In-memory inpaint mask aligned to the generation frame (255 = regenerate region).
#[derive(Debug, Clone)]
pub struct InpaintMask {
    pub width: u32,
    pub height: u32,
    pixels: Vec<u8>,
}

impl InpaintMask {
    pub fn new(width: u32, height: u32) -> Self {
        let n = (width as usize).saturating_mul(height as usize);
        Self {
            width,
            height,
            pixels: vec![0; n],
        }
    }

    pub fn ensure_size(&mut self, width: u32, height: u32) {
        if self.width != width || self.height != height {
            *self = Self::new(width, height);
        }
    }

    pub fn has_paint(&self) -> bool {
        self.pixels.iter().any(|&v| v > 0)
    }

    pub fn clear(&mut self) {
        self.pixels.fill(0);
    }

    pub fn paint_brush(&mut self, nx: f32, ny: f32, radius_norm: f32) {
        let cx = (nx.clamp(0.0, 1.0) * self.width as f32) as i32;
        let cy = (ny.clamp(0.0, 1.0) * self.height as f32) as i32;
        let r = (radius_norm * self.width.min(self.height) as f32).max(2.0) as i32;
        let r2 = r * r;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy > r2 {
                    continue;
                }
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }
                let idx = (y as u32 * self.width + x as u32) as usize;
                self.pixels[idx] = 255;
            }
        }
    }

    pub fn save_logical_png(&self, logical_path: &str) -> Result<(), String> {
        let host = logical_host_path(logical_path);
        if let Some(parent) = host.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut img = GrayImage::new(self.width, self.height);
        for (i, &v) in self.pixels.iter().enumerate() {
            let x = (i as u32) % self.width;
            let y = (i as u32) / self.width;
            img.put_pixel(x, y, Luma([v]));
        }
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        std::fs::write(&host, buf).map_err(|e| e.to_string())
    }
}

pub fn new_inpaint_mask_path() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("/downloads/inpaint-mask-{ts}.png")
}

fn logical_host_path(logical: &str) -> PathBuf {
    let trimmed = logical.trim();
    if trimmed.starts_with('/') {
        crate::os_open::aos_home()
            .join("var/storage/data")
            .join(trimmed.trim_start_matches('/'))
    } else {
        PathBuf::from(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inpaint_mask_paint_and_save() {
        let mut mask = InpaintMask::new(8, 8);
        assert!(!mask.has_paint());
        mask.paint_brush(0.5, 0.5, 0.25);
        assert!(mask.has_paint());
        let logical = format!("/downloads/test-mask-{}.png", std::process::id());
        mask.save_logical_png(&logical).expect("save mask");
        let host = logical_host_path(&logical);
        assert!(host.is_file());
        let _ = std::fs::remove_file(host);
    }

    #[test]
    fn ideogram_bbox_order() {
        let b = CompositionBlock {
            id: 1,
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4,
            desc: "cat".into(),
        };
        assert_eq!(b.ideogram_bbox(), [200, 100, 600, 400]);
    }

    #[test]
    fn overlap_preserved_in_elements() {
        let blocks = vec![
            CompositionBlock {
                id: 1,
                x: 0.0,
                y: 0.0,
                w: 0.8,
                h: 0.8,
                desc: "background tree".into(),
            },
            CompositionBlock {
                id: 2,
                x: 0.3,
                y: 0.3,
                w: 0.4,
                h: 0.4,
                desc: "person in front".into(),
            },
        ];
        let p = compose_prompt_with_layout("scene", &blocks, Some("local:ideogram-4"));
        let v: Value = serde_json::from_str(&p).unwrap();
        let els = v["compositional_deconstruction"]["elements"]
            .as_array()
            .unwrap();
        assert_eq!(els.len(), 2);
        assert_eq!(els[0]["desc"], "background tree");
        assert_eq!(els[1]["desc"], "person in front");
    }

    #[test]
    fn plain_fallback() {
        let blocks = vec![CompositionBlock {
            id: 1,
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4,
            desc: "vase".into(),
        }];
        let p = compose_prompt_with_layout("still life", &blocks, Some("local:unknown-model"));
        assert!(p.contains("still life"));
        assert!(p.contains("Composition (back to front)"));
        assert!(p.contains("vase"));
    }

    #[test]
    fn merge_recovers_background_from_destruction_typo() {
        let llm = r#"{"high_level_description":"alley","style_description":{"aesthetics":"cyber"},"compositional_destruction":{"background":"dark cyberpunk alley","elements":[{"type":"obj","desc":"a cat"}]}}"#;
        let blocks = vec![CompositionBlock {
            id: 1,
            x: 0.35,
            y: 0.5,
            w: 0.3,
            h: 0.3,
            desc: "studio cat with cyber paw".into(),
        }];
        let p = merge_layout_into_prompt(llm, &blocks, Some("local:ideogram4"));
        let v: Value = serde_json::from_str(&p).unwrap();
        assert!(v.get("compositional_destruction").is_none());
        assert_eq!(
            v["compositional_deconstruction"]["background"],
            "dark cyberpunk alley"
        );
        let els = v["compositional_deconstruction"]["elements"]
            .as_array()
            .unwrap();
        assert_eq!(els.len(), 1);
        assert_eq!(els[0]["desc"], "studio cat with cyber paw");
    }
}
