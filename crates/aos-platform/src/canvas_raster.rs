//! Rasterize a session canvas document to PNG (export snapshot, not diffusion).

use aos_proto::{
    canvas_layer_effective_opacity, canvas_layer_effective_visible, canvas_op_body_dash,
    canvas_op_body_gradient, canvas_op_body_opacity, sample_linear_gradient, CanvasAspect,
    CanvasDoc, CanvasOpBody, CanvasPoint, DEFAULT_CANVAS_LAYER_ID,
};
use image::{ImageBuffer, Rgb, RgbImage};
use std::cell::Cell;

thread_local! {
    static PAINT_OPACITY: Cell<f32> = const { Cell::new(1.0) };
}

const BG: Rgb<u8> = Rgb([7, 11, 20]); // void
const DEFAULT_FG: Rgb<u8> = Rgb([62, 224, 196]); // signal

pub fn export_png(doc: &CanvasDoc, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let w = width.max(64);
    let h = height.max(64);
    let mut img: RgbImage = ImageBuffer::from_pixel(w, h, BG);
    for op in &doc.ops {
        if !canvas_layer_effective_visible(doc, &op.layer_id) {
            continue;
        }
        let opacity =
            canvas_layer_effective_opacity(doc, &op.layer_id) * canvas_op_body_opacity(&op.body);
        if opacity <= 0.001 {
            continue;
        }
        paint_op(&mut img, &op.body, opacity);
    }
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// JSON sidecar next to a PNG export — full vector doc + aspect for reuse.
pub fn export_sidecar_json(doc: &CanvasDoc, aspect: CanvasAspect) -> Result<Vec<u8>, String> {
    let payload = serde_json::json!({
        "canvas_aspect": aspect,
        "session_id": doc.session_id,
        "next_seq": doc.next_seq,
        "ops": doc.ops,
        "pen": doc.pen,
        "layers": doc.layers,
        "active_layer_id": doc.active_layer_id,
        "next_layer_id": doc.next_layer_id,
    });
    serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())
}

/// `canvas-sess-123.png` or `.svg` → `canvas-sess-123.json`.
pub fn sidecar_path_for_export(export_path: &str) -> String {
    match export_path.rsplit_once('.') {
        Some((stem, ext))
            if ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("svg") =>
        {
            format!("{stem}.json")
        }
        _ => format!("{export_path}.json"),
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn svg_px(v: f32, span: u32) -> f32 {
    v.clamp(0.0, 1.0) * (span.saturating_sub(1) as f32)
}

fn svg_color(s: &str) -> String {
    if s.trim().is_empty() {
        "#3ee0c4".into()
    } else if s.starts_with('#') {
        s.to_string()
    } else {
        format!("#{s}")
    }
}

/// Vector SVG grouped by named layers. Flood-fill ops are omitted.
pub fn export_svg(doc: &CanvasDoc, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let w = width.max(64);
    let h = height.max(64);
    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\">"
    ));
    out.push_str(&format!(
        "<rect width=\"{w}\" height=\"{h}\" fill=\"#070b14\"/>"
    ));
    let layers = if doc.layers.is_empty() {
        vec![aos_proto::CanvasLayer::default()]
    } else {
        doc.layers.clone()
    };
    for layer in &layers {
        if !canvas_layer_effective_visible(doc, &layer.id) && !doc.layers.is_empty() {
            continue;
        }
        let opacity = if doc.layers.is_empty() {
            1.0
        } else {
            canvas_layer_effective_opacity(doc, &layer.id)
        };
        out.push_str(&format!(
            "<g id=\"{}\" data-name=\"{}\" opacity=\"{:.3}\">",
            xml_escape(&layer.id),
            xml_escape(&layer.name),
            opacity
        ));
        for op in &doc.ops {
            let lid = if op.layer_id.is_empty() {
                DEFAULT_CANVAS_LAYER_ID
            } else {
                op.layer_id.as_str()
            };
            if lid != layer.id {
                continue;
            }
            append_svg_op(&mut out, &op.body, w, h);
        }
        out.push_str("</g>");
    }
    out.push_str("</svg>");
    Ok(out.into_bytes())
}

pub fn svg_path_for_png(png_path: &str) -> String {
    match png_path.rsplit_once('.') {
        Some((stem, ext)) if ext.eq_ignore_ascii_case("png") => format!("{stem}.svg"),
        _ => format!("{png_path}.svg"),
    }
}

fn svg_opacity_attr(body: &CanvasOpBody) -> String {
    let op = canvas_op_body_opacity(body);
    if op >= 0.999 {
        String::new()
    } else {
        format!(" opacity=\"{op:.3}\"")
    }
}

fn svg_dash_attr(body: &CanvasOpBody, w: u32, h: u32) -> String {
    let dash = canvas_op_body_dash(body);
    if dash.is_empty() {
        return String::new();
    }
    let scale = w.min(h) as f32;
    let vals: Vec<String> = dash
        .iter()
        .map(|d| format!("{:.2}", (d * scale).max(1.0)))
        .collect();
    format!(" stroke-dasharray=\"{}\"", vals.join(" "))
}

fn svg_fill_color(body: &CanvasOpBody, color: &str, cx: f32, cy: f32) -> String {
    if let Some(g) = canvas_op_body_gradient(body) {
        if let Some([r, g, b]) = sample_linear_gradient(g, cx, cy) {
            return format!("#{:02x}{:02x}{:02x}", r, g, b);
        }
    }
    svg_color(color)
}

fn append_svg_op(out: &mut String, body: &CanvasOpBody, w: u32, h: u32) {
    let op_attr = svg_opacity_attr(body);
    let dash_attr = svg_dash_attr(body, w, h);
    match body {
        CanvasOpBody::Rect {
            x,
            y,
            w: bw,
            h: bh,
            color,
            fill,
            width,
            rotation,
            ..
        } => {
            let x0 = svg_px(*x, w);
            let y0 = svg_px(*y, h);
            let ww = svg_px(*x + *bw, w) - x0;
            let hh = svg_px(*y + *bh, h) - y0;
            let cx = svg_px(*x + *bw * 0.5, w);
            let cy = svg_px(*y + *bh * 0.5, h);
            let rot = if rotation.abs() > 0.001 {
                format!(" transform=\"rotate({rotation:.2} {cx:.2} {cy:.2})\"")
            } else {
                String::new()
            };
            let fill_c = svg_fill_color(body, color, *x + bw * 0.5, *y + bh * 0.5);
            if *fill {
                out.push_str(&format!(
                    "<rect x=\"{x0:.2}\" y=\"{y0:.2}\" width=\"{ww:.2}\" height=\"{hh:.2}\" fill=\"{fill_c}\"{op_attr}{rot}/>",
                ));
            } else {
                out.push_str(&format!(
                    "<rect x=\"{x0:.2}\" y=\"{y0:.2}\" width=\"{ww:.2}\" height=\"{hh:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"{op_attr}{dash_attr}{rot}/>",
                    svg_color(color),
                    (*width * w.min(h) as f32 * 0.5).max(1.0)
                ));
            }
        }
        CanvasOpBody::Ellipse {
            x,
            y,
            w: bw,
            h: bh,
            color,
            fill,
            width,
            rotation,
            ..
        } => {
            let cx = svg_px(*x + *bw * 0.5, w);
            let cy = svg_px(*y + *bh * 0.5, h);
            let rx = (*bw * w as f32 * 0.5).abs().max(1.0);
            let ry = (*bh * h as f32 * 0.5).abs().max(1.0);
            let rot = if rotation.abs() > 0.001 {
                format!(" transform=\"rotate({rotation:.2} {cx:.2} {cy:.2})\"")
            } else {
                String::new()
            };
            let fill_c = svg_fill_color(body, color, *x + bw * 0.5, *y + bh * 0.5);
            if *fill {
                out.push_str(&format!(
                    "<ellipse cx=\"{cx:.2}\" cy=\"{cy:.2}\" rx=\"{rx:.2}\" ry=\"{ry:.2}\" fill=\"{fill_c}\"{op_attr}{rot}/>",
                ));
            } else {
                out.push_str(&format!(
                    "<ellipse cx=\"{cx:.2}\" cy=\"{cy:.2}\" rx=\"{rx:.2}\" ry=\"{ry:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"{op_attr}{dash_attr}{rot}/>",
                    svg_color(color),
                    (*width * w.min(h) as f32 * 0.5).max(1.0)
                ));
            }
        }
        CanvasOpBody::Line { p0, p1, color, width, .. } => {
            out.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\"{op_attr}{dash_attr}/>",
                svg_px(p0.x, w),
                svg_px(p0.y, h),
                svg_px(p1.x, w),
                svg_px(p1.y, h),
                svg_color(color),
                (*width * w.min(h) as f32 * 0.5).max(1.0)
            ));
        }
        CanvasOpBody::Stroke { points, color, width, .. }
        | CanvasOpBody::Spline { points, color, width, .. } => {
            if points.len() < 2 {
                return;
            }
            let sampled = if matches!(body, CanvasOpBody::Spline { .. }) {
                sample_spline(points, 24)
            } else {
                points.clone()
            };
            let d = svg_poly_d(&sampled, w, h, false);
            out.push_str(&format!(
                "<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{op_attr}{dash_attr}/>",
                svg_color(color),
                (*width * w.min(h) as f32 * 0.5).max(1.0)
            ));
        }
        CanvasOpBody::Erase { points, width } => {
            if points.len() < 2 {
                return;
            }
            let d = svg_poly_d(points, w, h, false);
            out.push_str(&format!(
                "<path d=\"{d}\" fill=\"none\" stroke=\"#070b14\" stroke-width=\"{:.2}\" stroke-linecap=\"round\"/>",
                (*width * w.min(h) as f32 * 0.5).max(1.0)
            ));
        }
        CanvasOpBody::Path {
            points,
            color,
            width,
            fill,
            closed,
            ..
        } => {
            if points.len() < 2 {
                return;
            }
            let sampled = sample_spline(points, 24);
            let d = svg_poly_d(&sampled, w, h, *closed);
            let center = sampled.first().copied().unwrap_or(CanvasPoint { x: 0.5, y: 0.5 });
            let fill_attr = if *fill {
                svg_fill_color(body, color, center.x, center.y)
            } else {
                "none".into()
            };
            let stroke_w = if *width > 0.0 {
                (*width * w.min(h) as f32 * 0.5).max(1.0)
            } else {
                0.0
            };
            if stroke_w > 0.0 {
                out.push_str(&format!(
                    "<path d=\"{d}\" fill=\"{fill_attr}\" stroke=\"{}\" stroke-width=\"{stroke_w:.2}\"{op_attr}{dash_attr}/>",
                    svg_color(color)
                ));
            } else {
                out.push_str(&format!(
                    "<path d=\"{d}\" fill=\"{fill_attr}\"{op_attr}/>"
                ));
            }
        }
        CanvasOpBody::Fill { .. } | CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
}

fn svg_poly_d(points: &[CanvasPoint], w: u32, h: u32, closed: bool) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let cmd = if i == 0 { "M" } else { "L" };
        d.push_str(&format!("{cmd}{:.2},{:.2} ", svg_px(p.x, w), svg_px(p.y, h)));
    }
    if closed {
        d.push('Z');
    }
    d
}

fn png_rgb(body: &CanvasOpBody, color: &str, cx: f32, cy: f32) -> Rgb<u8> {
    if let Some(g) = canvas_op_body_gradient(body) {
        if let Some([r, g, b]) = sample_linear_gradient(g, cx, cy) {
            return Rgb([r, g, b]);
        }
    }
    parse_color(color).unwrap_or(DEFAULT_FG)
}

// TRUNCATED_FOR_TOOL_CALL
