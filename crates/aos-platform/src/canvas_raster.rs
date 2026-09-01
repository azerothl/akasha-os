//! Rasterize a session canvas document to PNG (export snapshot, not diffusion).

use aos_proto::{
    canvas_layer_effective_opacity, canvas_layer_effective_visible, canvas_op_body_opacity,
    CanvasAspect, CanvasDoc, CanvasOpBody, CanvasPoint, DEFAULT_CANVAS_LAYER_ID,
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

fn append_svg_op(out: &mut String, body: &CanvasOpBody, w: u32, h: u32) {
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
        .. } => {
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
            if *fill {
                out.push_str(&format!(
                    "<rect x=\"{x0:.2}\" y=\"{y0:.2}\" width=\"{ww:.2}\" height=\"{hh:.2}\" fill=\"{}\"{rot}/>",
                    svg_color(color)
                ));
            } else {
                out.push_str(&format!(
                    "<rect x=\"{x0:.2}\" y=\"{y0:.2}\" width=\"{ww:.2}\" height=\"{hh:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"{rot}/>",
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
        .. } => {
            let cx = svg_px(*x + *bw * 0.5, w);
            let cy = svg_px(*y + *bh * 0.5, h);
            let rx = (*bw * w as f32 * 0.5).abs().max(1.0);
            let ry = (*bh * h as f32 * 0.5).abs().max(1.0);
            let rot = if rotation.abs() > 0.001 {
                format!(" transform=\"rotate({rotation:.2} {cx:.2} {cy:.2})\"")
            } else {
                String::new()
            };
            if *fill {
                out.push_str(&format!(
                    "<ellipse cx=\"{cx:.2}\" cy=\"{cy:.2}\" rx=\"{rx:.2}\" ry=\"{ry:.2}\" fill=\"{}\"{rot}/>",
                    svg_color(color)
                ));
            } else {
                out.push_str(&format!(
                    "<ellipse cx=\"{cx:.2}\" cy=\"{cy:.2}\" rx=\"{rx:.2}\" ry=\"{ry:.2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\"{rot}/>",
                    svg_color(color),
                    (*width * w.min(h) as f32 * 0.5).max(1.0)
                ));
            }
        }
        CanvasOpBody::Line { p0, p1, color, width, .. } => {
            out.push_str(&format!(
                "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\"/>",
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
                "<path d=\"{d}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
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
        .. } => {
            if points.len() < 2 {
                return;
            }
            let sampled = sample_spline(points, 24);
            let d = svg_poly_d(&sampled, w, h, *closed);
            let fill_attr = if *fill {
                svg_color(color)
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
                    "<path d=\"{d}\" fill=\"{fill_attr}\" stroke=\"{}\" stroke-width=\"{stroke_w:.2}\"/>",
                    svg_color(color)
                ));
            } else {
                out.push_str(&format!("<path d=\"{d}\" fill=\"{fill_attr}\"/>"));
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

fn paint_op(img: &mut RgbImage, body: &CanvasOpBody, opacity: f32) {
    PAINT_OPACITY.with(|c| c.set(opacity.clamp(0.0, 1.0)));
    match body {
        CanvasOpBody::Stroke {
            points,
            color,
            width,
        .. } => {
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let rad = radius(img, *width);
            stroke_polyline(img, points, c, rad);
        }
        CanvasOpBody::Erase { points, width } => {
            let rad = radius(img, *width);
            PAINT_OPACITY.with(|c| c.set(1.0));
            stroke_polyline(img, points, BG, rad);
        }
        CanvasOpBody::Rect {
            x,
            y,
            w,
            h,
            color,
            fill,
            width,
            rotation,
        .. } => {
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            if rotation.abs() > 0.001 {
                let corners = aos_proto::canvas_rect_corners(*x, *y, *w, *h, *rotation);
                let pts: Vec<(i32, i32)> = corners
                    .iter()
                    .map(|(px, py)| to_px(img, *px, *py))
                    .collect();
                if *fill {
                    fill_polygon(img, &pts, c);
                } else {
                    let rad = radius(img, *width).max(1);
                    stroke_closed_i32(img, &pts, c, rad);
                }
            } else {
                let (x0, y0) = to_px(img, *x, *y);
                let (x1, y1) = to_px(img, x + w, y + h);
                if *fill {
                    fill_rect(img, x0, y0, x1, y1, c);
                } else {
                    let rad = radius(img, *width).max(1);
                    stroke_rect(img, x0, y0, x1, y1, c, rad);
                }
            }
        }
        CanvasOpBody::Ellipse {
            x,
            y,
            w,
            h,
            color,
            fill,
            width,
            rotation,
        .. } => {
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let cx = x + w * 0.5;
            let cy = y + h * 0.5;
            if rotation.abs() > 0.001 {
                let pts: Vec<(i32, i32)> = (0..48)
                    .map(|i| {
                        let t = std::f32::consts::TAU * (i as f32) / 48.0;
                        let px = cx + (w.abs() * 0.5) * t.cos();
                        let py = cy + (h.abs() * 0.5) * t.sin();
                        let (rx, ry) = aos_proto::canvas_rotate_point(cx, cy, px, py, *rotation);
                        to_px(img, rx, ry)
                    })
                    .collect();
                if *fill {
                    fill_polygon(img, &pts, c);
                } else {
                    let rad = radius(img, *width).max(1);
                    stroke_closed_i32(img, &pts, c, rad);
                }
            } else {
                let (cxp, cyp) = to_px(img, cx, cy);
                let rx = ((w.abs() * img.width() as f32) * 0.5).max(1.0) as i32;
                let ry = ((h.abs() * img.height() as f32) * 0.5).max(1.0) as i32;
                if *fill {
                    fill_ellipse(img, cxp, cyp, rx, ry, c);
                } else {
                    let rad = radius(img, *width).max(1);
                    stroke_ellipse(img, cxp, cyp, rx, ry, c, rad);
                }
            }
        }
        CanvasOpBody::Line { p0, p1, color, width, .. } => {
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let rad = radius(img, *width);
            let (x0, y0) = to_px(img, p0.x, p0.y);
            let (x1, y1) = to_px(img, p1.x, p1.y);
            line(img, x0, y0, x1, y1, rad, c);
        }
        CanvasOpBody::Spline { points, color, width, .. } => {
            if points.len() < 2 {
                return;
            }
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let rad = radius(img, *width);
            let sampled = sample_spline(points, 24);
            stroke_polyline(img, &sampled, c, rad);
        }
        CanvasOpBody::Path {
            points,
            color,
            width,
            fill,
            closed,
        .. } => {
            if points.len() < 2 {
                return;
            }
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let sampled = sample_spline(points, 24);
            let mut pts: Vec<(i32, i32)> = sampled.iter().map(|p| to_px(img, p.x, p.y)).collect();
            if *closed && pts.len() >= 3 {
                if let (Some(first), Some(last)) = (pts.first().copied(), pts.last().copied()) {
                    if first != last {
                        pts.push(first);
                    }
                }
            }
            if *fill && pts.len() >= 3 {
                fill_polygon(img, &pts, c);
            }
            if *width > 0.0 {
                let rad = radius(img, *width);
                stroke_polyline(img, &sampled, c, rad);
            }
        }
        CanvasOpBody::Fill { x, y, color, .. } => {
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let (px, py) = to_px(img, *x, *y);
            flood_fill(img, px, py, c);
        }
        CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
}

fn radius(img: &RgbImage, width: f32) -> i32 {
    let side = img.width().min(img.height()) as f32;
    ((width.clamp(0.001, 0.25) * side) * 0.5).round().max(1.0) as i32
}

fn to_px(img: &RgbImage, x: f32, y: f32) -> (i32, i32) {
    let px = (x.clamp(0.0, 1.0) * (img.width().saturating_sub(1) as f32)).round() as i32;
    let py = (y.clamp(0.0, 1.0) * (img.height().saturating_sub(1) as f32)).round() as i32;
    (px, py)
}

fn parse_color(s: &str) -> Option<Rgb<u8>> {
    let t = s.trim().trim_start_matches('#');
    if t.len() >= 6 {
        let r = u8::from_str_radix(&t[0..2], 16).ok()?;
        let g = u8::from_str_radix(&t[2..4], 16).ok()?;
        let b = u8::from_str_radix(&t[4..6], 16).ok()?;
        Some(Rgb([r, g, b]))
    } else {
        None
    }
}

fn put(img: &mut RgbImage, x: i32, y: i32, c: Rgb<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        let opacity = PAINT_OPACITY.with(|slot| slot.get());
        if opacity >= 0.999 {
            img.put_pixel(x as u32, y as u32, c);
            return;
        }
        let dst = img.get_pixel_mut(x as u32, y as u32);
        let a = opacity;
        dst[0] = (c[0] as f32 * a + dst[0] as f32 * (1.0 - a)).round() as u8;
        dst[1] = (c[1] as f32 * a + dst[1] as f32 * (1.0 - a)).round() as u8;
        dst[2] = (c[2] as f32 * a + dst[2] as f32 * (1.0 - a)).round() as u8;
    }
}

fn disc(img: &mut RgbImage, cx: i32, cy: i32, r: i32, c: Rgb<u8>) {
    let r2 = r * r;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r2 {
                put(img, cx + dx, cy + dy, c);
            }
        }
    }
}

fn stroke_closed_i32(img: &mut RgbImage, pts: &[(i32, i32)], c: Rgb<u8>, rad: i32) {
    if pts.len() < 2 {
        return;
    }
    for i in 0..pts.len() {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % pts.len()];
        line(img, x0, y0, x1, y1, rad, c);
    }
}

fn stroke_polyline(img: &mut RgbImage, points: &[CanvasPoint], c: Rgb<u8>, rad: i32) {
    if points.is_empty() {
        return;
    }
    let pts: Vec<(i32, i32)> = points
        .iter()
        .map(|p| to_px(img, p.x, p.y))
        .collect();
    disc(img, pts[0].0, pts[0].1, rad, c);
    for win in pts.windows(2) {
        line(img, win[0].0, win[0].1, win[1].0, win[1].1, rad, c);
    }
}

fn line(img: &mut RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, rad: i32, c: Rgb<u8>) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let steps = dx.max(dy).max(1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = x0 as f32 + (x1 - x0) as f32 * t;
        let y = y0 as f32 + (y1 - y0) as f32 * t;
        disc(img, x.round() as i32, y.round() as i32, rad, c);
    }
}

fn fill_rect(img: &mut RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgb<u8>) {
    let (xa, xb) = (x0.min(x1), x0.max(x1));
    let (ya, yb) = (y0.min(y1), y0.max(y1));
    for y in ya..=yb {
        for x in xa..=xb {
            put(img, x, y, c);
        }
    }
}

fn stroke_rect(img: &mut RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, c: Rgb<u8>, rad: i32) {
    line(img, x0, y0, x1, y0, rad, c);
    line(img, x1, y0, x1, y1, rad, c);
    line(img, x1, y1, x0, y1, rad, c);
    line(img, x0, y1, x0, y0, rad, c);
}

fn fill_ellipse(img: &mut RgbImage, cx: i32, cy: i32, rx: i32, ry: i32, c: Rgb<u8>) {
    let rx = rx.max(1);
    let ry = ry.max(1);
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            let nx = dx as f32 / rx as f32;
            let ny = dy as f32 / ry as f32;
            if nx * nx + ny * ny <= 1.0 {
                put(img, cx + dx, cy + dy, c);
            }
        }
    }
}

fn stroke_ellipse(img: &mut RgbImage, cx: i32, cy: i32, rx: i32, ry: i32, c: Rgb<u8>, rad: i32) {
    let steps = ((rx + ry) * 4).max(32);
    let mut prev: Option<(i32, i32)> = None;
    for i in 0..=steps {
        let t = std::f32::consts::TAU * (i as f32) / (steps as f32);
        let x = cx as f32 + rx as f32 * t.cos();
        let y = cy as f32 + ry as f32 * t.sin();
        let p = (x.round() as i32, y.round() as i32);
        if let Some(prev) = prev {
            line(img, prev.0, prev.1, p.0, p.1, rad, c);
        }
        prev = Some(p);
    }
}

fn sample_spline(points: &[CanvasPoint], segments_per_span: usize) -> Vec<CanvasPoint> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let mut out = Vec::new();
    let n = points.len();
    for i in 0..n.saturating_sub(1) {
        let p0 = if i == 0 { points[0] } else { points[i - 1] };
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = if i + 2 < n { points[i + 2] } else { points[i + 1] };
        let steps = segments_per_span.max(4);
        let start_j = if i == 0 { 0 } else { 1 };
        for j in start_j..=steps {
            let t = j as f32 / steps as f32;
            out.push(catmull_rom(p0, p1, p2, p3, t));
        }
    }
    out
}

fn catmull_rom(p0: CanvasPoint, p1: CanvasPoint, p2: CanvasPoint, p3: CanvasPoint, t: f32) -> CanvasPoint {
    let t2 = t * t;
    let t3 = t2 * t;
    CanvasPoint {
        x: 0.5
            * ((2.0 * p1.x)
                + (-p0.x + p2.x) * t
                + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3),
        y: 0.5
            * ((2.0 * p1.y)
                + (-p0.y + p2.y) * t
                + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3),
    }
}

/// Scanline fill for a closed polygon (pixel coords).
fn fill_polygon(img: &mut RgbImage, pts: &[(i32, i32)], c: Rgb<u8>) {
    if pts.len() < 3 {
        return;
    }
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for &(_, y) in pts {
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let w = img.width() as i32;
    let h = img.height() as i32;
    for y in min_y..=max_y {
        if y < 0 || y >= h {
            continue;
        }
        let mut crossings: Vec<i32> = Vec::new();
        for i in 0..pts.len().saturating_sub(1) {
            let (x0, y0) = pts[i];
            let (x1, y1) = pts[i + 1];
            if y0 == y1 {
                continue;
            }
            let ya = y0.min(y1);
            let yb = y0.max(y1);
            if y < ya || y >= yb {
                continue;
            }
            let t = (y - y0) as f32 / (y1 - y0) as f32;
            let x = x0 as f32 + (x1 - x0) as f32 * t;
            crossings.push(x.round() as i32);
        }
        crossings.sort_unstable();
        let mut i = 0;
        while i + 1 < crossings.len() {
            let x_start = crossings[i].max(0);
            let x_end = crossings[i + 1].min(w - 1);
            for x in x_start..=x_end {
                put(img, x, y, c);
            }
            i += 2;
        }
    }
}

fn flood_fill(img: &mut RgbImage, sx: i32, sy: i32, c: Rgb<u8>) {
    let w = img.width() as i32;
    let h = img.height() as i32;
    if sx < 0 || sy < 0 || sx >= w || sy >= h {
        return;
    }
    let target = *img.get_pixel(sx as u32, sy as u32);
    if target == c {
        return;
    }
    let mut stack = vec![(sx, sy)];
    while let Some((x, y)) = stack.pop() {
        if x < 0 || y < 0 || x >= w || y >= h {
            continue;
        }
        let px = img.get_pixel(x as u32, y as u32);
        if *px != target {
            continue;
        }
        img.put_pixel(x as u32, y as u32, c);
        stack.push((x + 1, y));
        stack.push((x - 1, y));
        stack.push((x, y + 1));
        stack.push((x, y - 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{CanvasOp, CanvasPenStyle, CanvasPoint};

    #[test]
    fn export_stroke_png_nonempty() {
        let doc = CanvasDoc {
            session_id: "s".into(),
            next_seq: 2,
            pen: CanvasPenStyle::default(),
            ops: vec![CanvasOp {
                seq: 1,
                author_id: "human".into(),
                ts_ms: 1,
                layer_id: String::new(),
                body: CanvasOpBody::Stroke {
                    points: vec![
                        CanvasPoint { x: 0.1, y: 0.1 },
                        CanvasPoint { x: 0.9, y: 0.9 },
                    ],
                    color: "#3ee0c4".into(),
                    width: 0.03,
                    opacity: 1.0,
                    dash: vec![],
                },
            }],
            ..Default::default()
        };
        let png = export_png(&doc, 128, 128).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.len() > 64);
        let sidecar = export_sidecar_json(&doc, CanvasAspect::Square).unwrap();
        let text = String::from_utf8(sidecar).unwrap();
        assert!(text.contains("\"session_id\": \"s\""));
        assert!(text.contains("\"canvas_aspect\""));
        assert!(text.contains("\"ops\""));
        let (parsed, aspect) =
            aos_proto::parse_canvas_sidecar_json(&text).expect("sidecar round-trip");
        assert_eq!(aspect, CanvasAspect::Square);
        assert_eq!(parsed.session_id, "s");
        assert_eq!(parsed.ops.len(), 1);
    }

    #[test]
    fn sidecar_path_replaces_export_extension() {
        assert_eq!(
            sidecar_path_for_export("/downloads/canvas-abc-1.png"),
            "/downloads/canvas-abc-1.json"
        );
        assert_eq!(
            sidecar_path_for_export("/downloads/canvas-abc-1.svg"),
            "/downloads/canvas-abc-1.json"
        );
        assert_eq!(sidecar_path_for_export("board"), "board.json");
    }

    #[test]
    fn export_svg_groups_layers() {
        let mut doc = CanvasDoc {
            session_id: "s".into(),
            next_seq: 2,
            pen: CanvasPenStyle::default(),
            ops: vec![CanvasOp {
                seq: 1,
                author_id: "human".into(),
                ts_ms: 1,
                layer_id: "lyr-1".into(),
                body: CanvasOpBody::Rect {
                    x: 0.1,
                    y: 0.1,
                    w: 0.2,
                    h: 0.2,
                    color: "#3ee0c4".into(),
                    fill: true,
                    width: 0.01,
                    rotation: 0.0,
                    opacity: 1.0,
                    dash: vec![],
                    gradient: None,
                },
            }],
            ..Default::default()
        };
        aos_proto::ensure_canvas_layers(&mut doc);
        let svg = String::from_utf8(export_svg(&doc, 128, 128).unwrap()).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("lyr-1"));
        assert!(svg.contains("<rect"));
    }

    #[test]
    fn export_windmill_paths_png_nonempty() {
        let hill = CanvasOpBody::Path {
            points: vec![
                CanvasPoint { x: 0.0, y: 0.92 },
                CanvasPoint { x: 0.15, y: 0.72 },
                CanvasPoint { x: 0.35, y: 0.68 },
                CanvasPoint { x: 0.50, y: 0.70 },
                CanvasPoint { x: 0.65, y: 0.68 },
                CanvasPoint { x: 0.85, y: 0.72 },
                CanvasPoint { x: 1.0, y: 0.92 },
            ],
            color: "#8B7355".into(),
            width: 0.0,
            fill: true,
            closed: true,
            opacity: 1.0,
            dash: vec![],
            gradient: None,
        };
        let body = CanvasOpBody::Path {
            points: vec![
                CanvasPoint { x: 0.44, y: 0.70 },
                CanvasPoint { x: 0.56, y: 0.70 },
                CanvasPoint { x: 0.54, y: 0.42 },
                CanvasPoint { x: 0.46, y: 0.42 },
            ],
            color: "#C4A574".into(),
            width: 0.0,
            fill: true,
            closed: true,
            opacity: 1.0,
            dash: vec![],
            gradient: None,
        };
        let roof = CanvasOpBody::Path {
            points: vec![
                CanvasPoint { x: 0.38, y: 0.44 },
                CanvasPoint { x: 0.50, y: 0.28 },
                CanvasPoint { x: 0.62, y: 0.44 },
            ],
            color: "#8B4513".into(),
            width: 0.0,
            fill: true,
            closed: true,
            opacity: 1.0,
            dash: vec![],
            gradient: None,
        };
        let sail_a = CanvasOpBody::Path {
            points: vec![
                CanvasPoint { x: 0.50, y: 0.36 },
                CanvasPoint { x: 0.22, y: 0.18 },
                CanvasPoint { x: 0.50, y: 0.30 },
                CanvasPoint { x: 0.78, y: 0.18 },
            ],
            color: "#E8DCC8".into(),
            width: 0.0,
            fill: true,
            closed: true,
            opacity: 1.0,
            dash: vec![],
            gradient: None,
        };
        let doc = CanvasDoc {
            session_id: "windmill".into(),
            next_seq: 5,
            pen: CanvasPenStyle::default(),
            ops: vec![
                CanvasOp {
                    seq: 1,
                    author_id: "agent".into(),
                    ts_ms: 1,
                    layer_id: String::new(),
                    body: hill,
                },
                CanvasOp {
                    seq: 2,
                    author_id: "agent".into(),
                    ts_ms: 2,
                    layer_id: String::new(),
                    body,
                },
                CanvasOp {
                    seq: 3,
                    author_id: "agent".into(),
                    ts_ms: 3,
                    layer_id: String::new(),
                    body: roof,
                },
                CanvasOp {
                    seq: 4,
                    author_id: "agent".into(),
                    ts_ms: 4,
                    layer_id: String::new(),
                    body: sail_a,
                },
            ],
            ..Default::default()
        };
        let png = export_png(&doc, 512, 512).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.len() > 1024);
        let out_dir = std::path::Path::new("/opt/cursor/artifacts");
        let _ = std::fs::create_dir_all(out_dir);
        let _ = std::fs::write(out_dir.join("windmill-path-demo.png"), &png);
    }
}
