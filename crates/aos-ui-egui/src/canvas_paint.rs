//! Shared canvas paint-style resolution for egui rendering (WYSIWYG with export).

use aos_proto::{
    canvas_op_body_dash, canvas_op_body_gradient, canvas_op_body_opacity, sample_linear_gradient,
    CanvasLayer, CanvasOpBody, CanvasPoint,
};
use eframe::egui::epaint::{PathStroke, Shape};
use eframe::egui::{Color32, Painter, Pos2, Rect};

use crate::chat_room;

pub fn parse_hex_color(s: &str) -> Color32 {
    let t = s.trim().trim_start_matches('#');
    if t.len() >= 6 {
        let r = u8::from_str_radix(&t[0..2], 16).unwrap_or(62);
        let g = u8::from_str_radix(&t[2..4], 16).unwrap_or(224);
        let b = u8::from_str_radix(&t[4..6], 16).unwrap_or(196);
        Color32::from_rgb(r, g, b)
    } else {
        Color32::from_rgb(0x3e, 0xe0, 0xc4)
    }
}

pub fn color_to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

pub fn author_stroke_color(author_id: &str, stored: &str, dark: bool) -> Color32 {
    if stored.trim().is_empty() {
        let (r, g, b) = chat_room::speaker_color_rgb(author_id, dark);
        Color32::from_rgb(r, g, b)
    } else {
        parse_hex_color(stored)
    }
}

pub fn to_screen(rect: Rect, p: CanvasPoint) -> Pos2 {
    Pos2::new(
        rect.left() + p.x.clamp(0.0, 1.0) * rect.width(),
        rect.top() + p.y.clamp(0.0, 1.0) * rect.height(),
    )
}

pub fn radius_px(rect: Rect, width: f32) -> f32 {
    (width.clamp(0.001, 0.25) * rect.width().min(rect.height()) * 0.5).max(0.5)
}

pub fn with_alpha(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        c.r(),
        c.g(),
        c.b(),
        (c.a() as f32 * alpha.clamp(0.0, 1.0)).round() as u8,
    )
}

pub fn layer_effective_opacity(layers: &[CanvasLayer], layer_id: &str) -> f32 {
    if layers.is_empty() {
        return 1.0;
    }
    let mut id = Some(layer_id);
    let mut guard = 0;
    let mut opacity = 1.0_f32;
    while let Some(cur) = id {
        if guard > 32 {
            break;
        }
        guard += 1;
        let Some(layer) = layers.iter().find(|l| l.id == cur) else {
            return 0.0;
        };
        if !layer.visible {
            return 0.0;
        }
        opacity *= layer.opacity.clamp(0.0, 1.0);
        id = layer.parent_id.as_deref();
    }
    opacity
}

pub fn combined_opacity(body: &CanvasOpBody, layer_opacity: f32) -> f32 {
    layer_opacity * canvas_op_body_opacity(body)
}

pub fn stroke_color(
    body: &CanvasOpBody,
    author_id: &str,
    stored: &str,
    dark: bool,
    layer_opacity: f32,
) -> Color32 {
    let base = author_stroke_color(author_id, stored, dark);
    with_alpha(base, combined_opacity(body, layer_opacity))
}

pub fn fill_color(
    body: &CanvasOpBody,
    author_id: &str,
    stored: &str,
    dark: bool,
    layer_opacity: f32,
    center: CanvasPoint,
) -> Color32 {
    let opacity = combined_opacity(body, layer_opacity);
    if let Some(g) = canvas_op_body_gradient(body) {
        if let Some([r, g, b]) = sample_linear_gradient(g, center.x, center.y) {
            return with_alpha(Color32::from_rgb(r, g, b), opacity);
        }
    }
    stroke_color(body, author_id, stored, dark, layer_opacity)
}

pub fn path_stroke(width: f32, color: Color32) -> PathStroke {
    PathStroke::new(width, color)
}

fn dash_lengths_px(dash: &[f32], rect: Rect) -> Vec<f32> {
    let scale = rect.width().min(rect.height()).max(1.0);
    dash.iter().map(|d| (d * scale).max(1.0)).collect()
}

/// Draw a polyline; uses manual dash segmentation when `dash` is non-empty.
pub fn paint_polyline(
    painter: &Painter,
    rect: Rect,
    points: &[CanvasPoint],
    width: f32,
    color: Color32,
    dash: &[f32],
) {
    if points.len() < 2 {
        return;
    }
    let screen: Vec<Pos2> = points.iter().map(|p| to_screen(rect, *p)).collect();
    let stroke_w = radius_px(rect, width) * 2.0;
    if dash.is_empty() {
        painter.add(Shape::line(screen, path_stroke(stroke_w, color)));
        return;
    }
    let pattern = dash_lengths_px(dash, rect);
    if pattern.len() < 2 {
        painter.add(Shape::line(screen, path_stroke(stroke_w, color)));
        return;
    }
    let mut pat_i = 0;
    let mut draw = true;
    let mut remain = pattern[0];
    for window in screen.windows(2) {
        let a = window[0];
        let b = window[1];
        let seg_len = a.distance(b);
        if seg_len <= 0.001 {
            continue;
        }
        let dir = (b - a) / seg_len;
        let mut traveled = 0.0;
        while traveled < seg_len - 0.001 {
            let step = remain.min(seg_len - traveled);
            let p0 = a + dir * traveled;
            let p1 = a + dir * (traveled + step);
            if draw && step > 0.001 {
                painter.line_segment([p0, p1], eframe::egui::Stroke::new(stroke_w, color));
            }
            traveled += step;
            remain -= step;
            if remain <= 0.001 {
                pat_i = (pat_i + 1) % pattern.len();
                remain = pattern[pat_i];
                draw = !draw;
            }
        }
    }
}

pub fn body_dash(body: &CanvasOpBody) -> &[f32] {
    canvas_op_body_dash(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_opacity_multiplies() {
        let layers = vec![
            CanvasLayer {
                id: "lyr-1".into(),
                name: "A".into(),
                visible: true,
                locked: false,
                opacity: 0.5,
                parent_id: None,
            },
            CanvasLayer {
                id: "lyr-2".into(),
                name: "B".into(),
                visible: true,
                locked: false,
                opacity: 0.5,
                parent_id: Some("lyr-1".into()),
            },
        ];
        assert!((layer_effective_opacity(&layers, "lyr-2") - 0.25).abs() < 1e-6);
    }

    #[test]
    fn combined_opacity_uses_body_alpha() {
        let body = CanvasOpBody::Line {
            p0: CanvasPoint { x: 0.0, y: 0.0 },
            p1: CanvasPoint { x: 1.0, y: 1.0 },
            color: "#ffffff".into(),
            width: 0.01,
            opacity: 0.5,
            dash: vec![],
        };
        assert!((combined_opacity(&body, 0.8) - 0.4).abs() < 1e-6);
    }
}
