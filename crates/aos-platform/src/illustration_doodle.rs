//! Doodle look: a found photo is the subject, ink is drawn on top of it.

use aos_proto::{IllustrationPaletteColors, IllustrationPart, IllustrationSpec};
use image::{Rgb, RgbImage};

pub fn paint_photo(img: &mut RgbImage, path: &str, subject: &str, colors: &IllustrationPaletteColors) {
    let night = {
        let s = subject.to_ascii_lowercase();
        s.contains("nuit") || s.contains("night")
    };
    let loaded = if path.is_empty() {
        None
    } else {
        image::open(path).ok().map(|im| im.to_rgb8())
    };
    if let Some(photo) = loaded {
        blit_contain(img, &photo);
    } else {
        photo_frame(img, colors);
    }
    if night {
        let night_c = parse_hex(&colors.night);
        for px in img.pixels_mut() {
            *px = mix(*px, night_c, 0.55);
        }
    }
}

pub fn paint_glow(img: &mut RgbImage, spec: &IllustrationSpec, colors: &IllustrationPaletteColors) {
    let glow = parse_hex(&colors.light);
    for part in &spec.parts {
        if !is_glow(part) {
            continue;
        }
        let (cx, cy, r) = glow_disk(part, img.width(), img.height());
        let r = r.max(8.0);
        let r2 = r * r;
        let x0 = (cx - r).floor().max(0.0) as u32;
        let y0 = (cy - r).floor().max(0.0) as u32;
        let x1 = (cx + r).ceil().min(img.width() as f32) as u32;
        let y1 = (cy + r).ceil().min(img.height() as f32) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let d2 = (x as f32 - cx).powi(2) + (y as f32 - cy).powi(2);
                if d2 < r2 {
                    let al = (1.0 - d2 / r2) * 0.45;
                    let p = img.get_pixel(x, y);
                    img.put_pixel(x, y, mix(*p, glow, al));
                }
            }
        }
    }
}

fn is_glow(part: &IllustrationPart) -> bool {
    let id = part.id.to_ascii_lowercase();
    id.contains("flame")
        || id.contains("flamme")
        || id.contains("lamp")
        || id.contains("lampe")
        || id.contains("glow")
        || id.contains("light")
}

fn glow_disk(part: &IllustrationPart, w: u32, h: u32) -> (f32, f32, f32) {
    match &part.geometry {
        aos_proto::IllustrationPartGeometry::Ellipse { x, y, w: pw, h: ph, .. }
        | aos_proto::IllustrationPartGeometry::Rect { x, y, w: pw, h: ph, .. } => (
            (x + pw * 0.5) * w as f32,
            (y + ph * 0.5) * h as f32,
            pw.max(*ph) * w as f32 * 1.4,
        ),
        aos_proto::IllustrationPartGeometry::Path { points, .. } => {
            if points.is_empty() {
                return (w as f32 * 0.5, h as f32 * 0.4, w as f32 * 0.08);
            }
            let cx = points.iter().map(|p| p.x).sum::<f32>() / points.len() as f32;
            let cy = points.iter().map(|p| p.y).sum::<f32>() / points.len() as f32;
            (cx * w as f32, cy * h as f32, w as f32 * 0.08)
        }
    }
}

fn blit_contain(dst: &mut RgbImage, src: &RgbImage) {
    let dw = dst.width() as f32;
    let dh = dst.height() as f32;
    let margin = 0.12;
    let box_w = dw * (1.0 - margin * 2.0);
    let box_h = dh * (1.0 - margin * 2.0);
    let scale = (box_w / src.width() as f32).min(box_h / src.height() as f32);
    let sw = (src.width() as f32 * scale).max(1.0);
    let sh = (src.height() as f32 * scale).max(1.0);
    let ox = ((dw - sw) * 0.5) as u32;
    let oy = ((dh - sh) * 0.5) as u32;
    for y in 0..sh as u32 {
        for x in 0..sw as u32 {
            let sx = (x as f32 / sw * src.width() as f32) as u32;
            let sy = (y as f32 / sh * src.height() as f32) as u32;
            let sx = sx.min(src.width() - 1);
            let sy = sy.min(src.height() - 1);
            let dx = ox + x;
            let dy = oy + y;
            if dx < dst.width() && dy < dst.height() {
                dst.put_pixel(dx, dy, *src.get_pixel(sx, sy));
            }
        }
    }
}

fn photo_frame(img: &mut RgbImage, colors: &IllustrationPaletteColors) {
    let ink = parse_hex(&colors.ink);
    let w = img.width() as i32;
    let h = img.height() as i32;
    let x0 = w / 8;
    let y0 = h / 8;
    let x1 = w - w / 8;
    let y1 = h - h / 6;
    for x in x0..x1 {
        put(img, x, y0, ink);
        put(img, x, y1, ink);
        put(img, x, y0 + 3, ink);
        put(img, x, y1 - 3, ink);
    }
    for y in y0..y1 {
        put(img, x0, y, ink);
        put(img, x1, y, ink);
        put(img, x0 + 3, y, ink);
        put(img, x1 - 3, y, ink);
    }
}

fn put(img: &mut RgbImage, x: i32, y: i32, c: Rgb<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, c);
    }
}

fn parse_hex(s: &str) -> Rgb<u8> {
    let s = s.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(&s.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    if s.len() >= 6 {
        Rgb([byte(0), byte(2), byte(4)])
    } else {
        Rgb([30, 22, 48])
    }
}

fn mix(a: Rgb<u8>, b: Rgb<u8>, t: f32) -> Rgb<u8> {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb([m(a[0], b[0]), m(a[1], b[1]), m(a[2], b[2])])
}
