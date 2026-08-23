//! Rasterize a session canvas document to PNG (export snapshot, not diffusion).

use aos_proto::{CanvasDoc, CanvasOpBody, CanvasPenStyle, CanvasPoint};
use image::{ImageBuffer, Rgb, RgbImage};

const BG: Rgb<u8> = Rgb([7, 11, 20]); // void
const DEFAULT_FG: Rgb<u8> = Rgb([62, 224, 196]); // signal

pub fn export_png(doc: &CanvasDoc, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let w = width.max(64);
    let h = height.max(64);
    let mut img: RgbImage = ImageBuffer::from_pixel(w, h, BG);
    for op in &doc.ops {
        paint_op(&mut img, &op.body);
    }
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn paint_op(img: &mut RgbImage, body: &CanvasOpBody) {
    match body {
        CanvasOpBody::Stroke {
            points,
            color,
            width,
        } => {
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let rad = radius(img, *width);
            stroke_polyline(img, points, c, rad);
        }
        CanvasOpBody::Erase { points, width } => {
            let rad = radius(img, *width);
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
        } => {
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let (x0, y0) = to_px(img, *x, *y);
            let (x1, y1) = to_px(img, x + w, y + h);
            if *fill {
                fill_rect(img, x0, y0, x1, y1, c);
            } else {
                let rad = radius(img, *width).max(1);
                stroke_rect(img, x0, y0, x1, y1, c, rad);
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
        } => {
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let (cx, cy) = to_px(img, x + w * 0.5, y + h * 0.5);
            let rx = ((w.abs() * img.width() as f32) * 0.5).max(1.0) as i32;
            let ry = ((h.abs() * img.height() as f32) * 0.5).max(1.0) as i32;
            if *fill {
                fill_ellipse(img, cx, cy, rx, ry, c);
            } else {
                let rad = radius(img, *width).max(1);
                stroke_ellipse(img, cx, cy, rx, ry, c, rad);
            }
        }
        CanvasOpBody::Line { p0, p1, color, width } => {
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let rad = radius(img, *width);
            let (x0, y0) = to_px(img, p0.x, p0.y);
            let (x1, y1) = to_px(img, p1.x, p1.y);
            line(img, x0, y0, x1, y1, rad, c);
        }
        CanvasOpBody::Spline { points, color, width } => {
            if points.len() < 2 {
                return;
            }
            let c = parse_color(color).unwrap_or(DEFAULT_FG);
            let rad = radius(img, *width);
            let sampled = sample_spline(points, 24);
            stroke_polyline(img, &sampled, c, rad);
        }
        CanvasOpBody::Fill { x, y, color } => {
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
        img.put_pixel(x as u32, y as u32, c);
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
    use aos_proto::{CanvasOp, CanvasPoint};

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
                body: CanvasOpBody::Stroke {
                    points: vec![
                        CanvasPoint { x: 0.1, y: 0.1 },
                        CanvasPoint { x: 0.9, y: 0.9 },
                    ],
                    color: "#3ee0c4".into(),
                    width: 0.03,
                },
            }],
        };
        let png = export_png(&doc, 128, 128).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.len() > 64);
    }
}
