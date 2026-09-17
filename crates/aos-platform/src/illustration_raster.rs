//! Illustration finish raster — paper, wob outlines, hatch / dot screens (skill-inspired).

use aos_proto::{
    IllustrationDoc, IllustrationFinish, IllustrationLook, IllustrationPaletteColors,
    IllustrationPart, IllustrationPartGeometry, IllustrationRenderMode, IllustrationSpec,
};
use image::{ImageBuffer, Rgb, RgbImage};
use std::f32::consts::TAU;

/// Seeded PRNG (xorshift-like), matching skill: no Math.random.
fn rng(seed: u32) -> impl FnMut() -> f32 {
    let mut a = seed.wrapping_mul(1_000_003);
    move || {
        a = a.wrapping_add(0x6D2B_79F5);
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as u32) as f32 / 4_294_967_296.0
    }
}

fn parse_hex(s: &str) -> Rgb<u8> {
    let t = s.trim().trim_start_matches('#');
    if t.len() >= 6 {
        let r = u8::from_str_radix(&t[0..2], 16).unwrap_or(30);
        let g = u8::from_str_radix(&t[2..4], 16).unwrap_or(22);
        let b = u8::from_str_radix(&t[4..6], 16).unwrap_or(48);
        Rgb([r, g, b])
    } else {
        Rgb([30, 22, 48])
    }
}

fn mix(a: Rgb<u8>, b: Rgb<u8>, t: f32) -> Rgb<u8> {
    let t = t.clamp(0.0, 1.0);
    Rgb([
        (a[0] as f32 * (1.0 - t) + b[0] as f32 * t).round() as u8,
        (a[1] as f32 * (1.0 - t) + b[1] as f32 * t).round() as u8,
        (a[2] as f32 * (1.0 - t) + b[2] as f32 * t).round() as u8,
    ])
}

fn shade(c: Rgb<u8>, t: f32) -> Rgb<u8> {
    mix(c, Rgb([0, 0, 0]), t)
}

fn put(img: &mut RgbImage, x: i32, y: i32, c: Rgb<u8>, alpha: f32) {
    let w = img.width() as i32;
    let h = img.height() as i32;
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.001 {
        return;
    }
    let px = img.get_pixel_mut(x as u32, y as u32);
    *px = mix(*px, c, a);
}

fn fill_rect(img: &mut RgbImage, color: Rgb<u8>) {
    for p in img.pixels_mut() {
        *p = color;
    }
}

/// Rasterize the current illustration still.
pub fn export_png(doc: &IllustrationDoc, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let w = width.max(64);
    let h = height.max(64);
    let palette_id = doc.brief.palette;
    let colors = IllustrationPaletteColors::for_preset(palette_id);
    let look = doc.brief.look;
    let mut img: RgbImage = ImageBuffer::from_pixel(w, h, parse_hex(&colors.paper));

    let night = look == IllustrationLook::Blueprint
        || doc
            .spec
            .as_ref()
            .map(|s| s.mode == IllustrationRenderMode::Blueprint)
            .unwrap_or(false);
    if night {
        fill_rect(&mut img, parse_hex(&colors.night));
        grain(&mut img, None, 400, Rgb([255, 255, 255]), 0.35, 5);
    } else {
        paper_stock(&mut img, &colors);
    }

    if let Some(spec) = doc.spec.as_ref() {
        paint_spec(&mut img, spec, &colors, night);
    }

    encode_png(&img)
}

/// Style sheet: palette swatches + subject at 0.6 / 1.0 / 1.8 scales.
pub fn export_sheet_png(doc: &IllustrationDoc, cell: u32) -> Result<Vec<u8>, String> {
    let cell = cell.max(160);
    let cols = 3u32;
    let rows = 2u32;
    let w = cell * cols;
    let h = cell * rows;
    let colors = IllustrationPaletteColors::for_preset(doc.brief.palette);
    let mut img: RgbImage = ImageBuffer::from_pixel(w, h, parse_hex(&colors.paper));
    paper_stock(&mut img, &colors);

    // Row 0: palette swatches
    let swatches: Vec<&str> = std::iter::once(colors.paper.as_str())
        .chain(colors.fills.iter().map(|s| s.as_str()))
        .chain(std::iter::once(colors.ink.as_str()))
        .take(8)
        .collect();
    let sw_w = (cell as f32 * 0.9 / swatches.len().max(1) as f32) as i32;
    for (i, hex) in swatches.iter().enumerate() {
        let x0 = (cell as i32 / 20) + i as i32 * (sw_w + 4);
        let y0 = cell as i32 / 6;
        fill_box(
            &mut img,
            x0,
            y0,
            sw_w,
            cell as i32 / 3,
            parse_hex(hex),
            1.0,
        );
    }

    // Row 0 right: finish label area — small sample hatch
    if let Some(spec) = doc.spec.as_ref() {
        let scales = [0.6f32, 1.0, 1.8];
        for (i, scale) in scales.iter().enumerate() {
            let ox = (i as u32 % cols) * cell;
            let oy = cell; // row 1
            blit_scaled_subject(&mut img, ox, oy, cell, spec, &colors, *scale);
        }
    }

    encode_png(&img)
}

fn blit_scaled_subject(
    img: &mut RgbImage,
    ox: u32,
    oy: u32,
    cell: u32,
    spec: &IllustrationSpec,
    colors: &IllustrationPaletteColors,
    scale: f32,
) {
    let sub_w = ((cell as f32) * 0.85) as u32;
    let sub_h = sub_w;
    let mut tile: RgbImage = ImageBuffer::from_pixel(sub_w, sub_h, parse_hex(&colors.paper));
    let mut scaled = spec.clone();
    // Keep camera; scale is applied by painting into a smaller logical frame via camera zoom.
    scaled.camera.zoom *= scale.clamp(0.3, 2.5);
    paint_spec(&mut tile, &scaled, colors, false);
    let pad = ((cell - sub_w) / 2) as i32;
    for y in 0..sub_h {
        for x in 0..sub_w {
            let c = *tile.get_pixel(x, y);
            put(
                img,
                ox as i32 + pad + x as i32,
                oy as i32 + pad + y as i32,
                c,
                1.0,
            );
        }
    }
}

fn paper_stock(img: &mut RgbImage, colors: &IllustrationPaletteColors) {
    fill_rect(img, parse_hex(&colors.paper));
    // Light diagonal bands
    let band = mix(parse_hex(&colors.paper), Rgb([255, 238, 200]), 0.35);
    let w = img.width() as i32;
    let h = img.height() as i32;
    for i in -6..=6 {
        let y0 = h / 2 + i * (h / 7) - h / 20;
        for y in y0..(y0 + h / 14) {
            for x in 0..w {
                // diagonal-ish: offset by x
                let yy = y + (x / 8);
                if yy >= 0 && yy < h {
                    put(img, x, yy, band, 0.12);
                }
            }
        }
    }
    grain(img, None, 1400, shade(parse_hex(&colors.paper), 0.5), 0.06, 5);
}

fn grain(img: &mut RgbImage, clip: Option<&[(i32, i32)]>, n: u32, color: Rgb<u8>, al: f32, seed: u32) {
    let mut r = rng(seed);
    let w = img.width() as i32;
    let h = img.height() as i32;
    for _ in 0..n {
        let x = (r() * w as f32) as i32;
        let y = (r() * h as f32) as i32;
        if let Some(poly) = clip {
            if !point_in_poly(x, y, poly) {
                continue;
            }
        }
        let s = 1 + (r() * 2.0) as i32;
        for dy in 0..s {
            for dx in 0..s {
                put(img, x + dx, y + dy, color, al);
            }
        }
    }
}

fn paint_spec(
    img: &mut RgbImage,
    spec: &IllustrationSpec,
    colors: &IllustrationPaletteColors,
    blueprint: bool,
) {
    let w = img.width() as f32;
    let h = img.height() as f32;
    let cam = &spec.camera;
    let finish = if blueprint {
        IllustrationFinish::Flat
    } else {
        colors.finish
    };
    let outline_color = if blueprint {
        parse_hex(&colors.chalk)
    } else {
        parse_hex(&colors.ink)
    };

    if spec.show_construction && !blueprint {
        construction(img, cam.x * w, cam.y * h, w.min(h) * 0.28, 11, parse_hex(&colors.guide));
    }

    for part in &spec.parts {
        let poly = part_poly(part, w, h, cam);
        if poly.len() < 3 {
            continue;
        }
        let fill_c = if blueprint {
            parse_hex(&colors.night)
        } else {
            let idx = part.fill_index as usize % colors.fills.len().max(1);
            parse_hex(colors.fills.get(idx).unwrap_or(&colors.fills[0]))
        };
        if part.fill && !blueprint {
            fill_poly(img, &poly, fill_c, 1.0);
            surface(img, &poly, finish, parse_hex(&colors.shade), part.seed);
        }
        if part.outline || blueprint {
            let amp = if blueprint { 1.2 } else { 2.2 };
            let width = if blueprint { 2.6 } else { 2.2 };
            wob_outline(img, &poly, outline_color, amp, width, part.seed.wrapping_add(7), true);
        }
        if spec.scribble_part.as_deref() == Some(part.id.as_str()) && !blueprint {
            scribble(img, &poly, colors, part.seed.wrapping_add(99));
        }
    }

    // Pose tilt: subtle — already baked if agent offsets geometry; apply global rotation hint via grain pulse
    let _ = (spec.pose.twitch, spec.pose.tilt);
}

fn part_poly(
    part: &IllustrationPart,
    w: f32,
    h: f32,
    cam: &aos_proto::IllustrationCamera,
) -> Vec<(i32, i32)> {
    let map = |nx: f32, ny: f32| -> (i32, i32) {
        // Camera: place (cam.x, cam.y) at centre, zoom.
        let cx = 0.5;
        let cy = 0.5;
        let zx = (nx - cam.x) * cam.zoom + cx;
        let zy = (ny - cam.y) * cam.zoom + cy;
        // rotation around centre
        let dx = zx - cx;
        let dy = zy - cy;
        let cos = cam.rot.cos();
        let sin = cam.rot.sin();
        let rx = cx + dx * cos - dy * sin;
        let ry = cy + dx * sin + dy * cos;
        ((rx * w).round() as i32, (ry * h).round() as i32)
    };
    match &part.geometry {
        IllustrationPartGeometry::Ellipse {
            x,
            y,
            w: bw,
            h: bh,
            rotation,
        } => {
            let cx = *x + *bw * 0.5;
            let cy = *y + *bh * 0.5;
            let rx = *bw * 0.5;
            let ry = *bh * 0.5;
            let n = 44;
            let mut pts = Vec::with_capacity(n);
            for i in 0..n {
                let a = i as f32 / n as f32 * TAU;
                let px = rx * a.cos();
                let py = ry * a.sin();
                let cos = rotation.cos();
                let sin = rotation.sin();
                let wx = cx + px * cos - py * sin;
                let wy = cy + px * sin + py * cos;
                pts.push(map(wx, wy));
            }
            pts
        }
        IllustrationPartGeometry::Rect {
            x,
            y,
            w: bw,
            h: bh,
            rotation,
        } => {
            let cx = *x + *bw * 0.5;
            let cy = *y + *bh * 0.5;
            let corners = [
                (*x, *y),
                (*x + *bw, *y),
                (*x + *bw, *y + *bh),
                (*x, *y + *bh),
            ];
            corners
                .into_iter()
                .map(|(px, py)| {
                    let dx = px - cx;
                    let dy = py - cy;
                    let cos = rotation.cos();
                    let sin = rotation.sin();
                    map(cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
                })
                .collect()
        }
        IllustrationPartGeometry::Path { points, .. } => {
            points.iter().map(|p| map(p.x, p.y)).collect()
        }
    }
}

fn fill_poly(img: &mut RgbImage, poly: &[(i32, i32)], color: Rgb<u8>, alpha: f32) {
    if poly.is_empty() {
        return;
    }
    let min_x = poly.iter().map(|p| p.0).min().unwrap().max(0);
    let max_x = poly
        .iter()
        .map(|p| p.0)
        .max()
        .unwrap()
        .min(img.width() as i32 - 1);
    let min_y = poly.iter().map(|p| p.1).min().unwrap().max(0);
    let max_y = poly
        .iter()
        .map(|p| p.1)
        .max()
        .unwrap()
        .min(img.height() as i32 - 1);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if point_in_poly(x, y, poly) {
                put(img, x, y, color, alpha);
            }
        }
    }
}

fn point_in_poly(x: i32, y: i32, poly: &[(i32, i32)]) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        let intersect = ((yi > y) != (yj > y))
            && ((x as f32)
                < (xj as f32 - xi as f32) * (y as f32 - yi as f32) / (yj as f32 - yi as f32 + 0.0)
                    + xi as f32);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn surface(
    img: &mut RgbImage,
    poly: &[(i32, i32)],
    finish: IllustrationFinish,
    shade_c: Rgb<u8>,
    seed: u32,
) {
    match finish {
        IllustrationFinish::Ink => {
            hatch(img, poly, 1.2, 4.5, 9.0, shade_c, 0.35, seed);
            grain(img, Some(poly), 140, shade_c, 0.3, seed.wrapping_add(1));
        }
        IllustrationFinish::Pencil => {
            hatch(img, poly, 1.1, 9.0, 30.0, shade_c, 0.22, seed);
            grain(img, Some(poly), 40, shade_c, 0.25, seed.wrapping_add(1));
        }
        IllustrationFinish::Riso => {
            dot_screen(img, poly, 7.0, shade_c, 0.55, 0.26, 0.35, seed);
        }
        IllustrationFinish::Screen => {
            dot_screen(img, poly, 6.0, shade_c, 0.5, 0.0, 0.06, seed);
        }
        IllustrationFinish::Flat => {}
    }
}

fn hatch(
    img: &mut RgbImage,
    poly: &[(i32, i32)],
    angle: f32,
    gap: f32,
    len: f32,
    color: Rgb<u8>,
    al: f32,
    seed: u32,
) {
    let mut r = rng(seed);
    let min_x = poly.iter().map(|p| p.0).min().unwrap_or(0) as f32;
    let max_x = poly.iter().map(|p| p.0).max().unwrap_or(0) as f32;
    let min_y = poly.iter().map(|p| p.1).min().unwrap_or(0) as f32;
    let max_y = poly.iter().map(|p| p.1).max().unwrap_or(0) as f32;
    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;
    let rad = ((max_x - min_x).hypot(max_y - min_y)) * 0.5;
    let ca = angle.cos();
    let sa = angle.sin();
    let mut v = -rad;
    while v <= rad {
        let mut u = -rad;
        while u <= rad {
            let uu = u + (r() - 0.5) * 6.0;
            let stroke_len = len * (0.6 + r() * 0.8);
            let x0 = cx + ca * uu - sa * v + (r() - 0.5) * 2.0;
            let y0 = cy + sa * uu + ca * v + (r() - 0.5) * 2.0;
            let x1 = x0 + ca * stroke_len;
            let y1 = y0 + sa * stroke_len;
            draw_line_clipped(img, poly, x0, y0, x1, y1, color, al);
            u += len * 1.7;
        }
        v += gap;
    }
}

fn dot_screen(
    img: &mut RgbImage,
    poly: &[(i32, i32)],
    cell: f32,
    color: Rgb<u8>,
    density: f32,
    angle: f32,
    jitter: f32,
    seed: u32,
) {
    let mut r = rng(seed);
    let min_x = poly.iter().map(|p| p.0).min().unwrap_or(0) as f32;
    let max_x = poly.iter().map(|p| p.0).max().unwrap_or(0) as f32;
    let min_y = poly.iter().map(|p| p.1).min().unwrap_or(0) as f32;
    let max_y = poly.iter().map(|p| p.1).max().unwrap_or(0) as f32;
    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;
    let rad = ((max_x - min_x).hypot(max_y - min_y)) * 0.5;
    let ca = angle.cos();
    let sa = angle.sin();
    let mut v = -rad;
    while v <= rad {
        let mut u = -rad;
        while u <= rad {
            let x = cx + ca * u - sa * v + (r() - 0.5) * jitter * cell;
            let y = cy + sa * u + ca * v + (r() - 0.5) * jitter * cell;
            let rad_d = cell * 0.62 * density.sqrt();
            if point_in_poly(x as i32, y as i32, poly) {
                fill_circle(img, x as i32, y as i32, rad_d.max(0.8) as i32, color, 0.9);
            }
            u += cell;
        }
        v += cell;
    }
}

fn fill_circle(img: &mut RgbImage, cx: i32, cy: i32, r: i32, color: Rgb<u8>, al: f32) {
    for y in (cy - r)..=(cy + r) {
        for x in (cx - r)..=(cx + r) {
            if (x - cx) * (x - cx) + (y - cy) * (y - cy) <= r * r {
                put(img, x, y, color, al);
            }
        }
    }
}

fn wob_outline(
    img: &mut RgbImage,
    poly: &[(i32, i32)],
    color: Rgb<u8>,
    amp: f32,
    width: f32,
    seed: u32,
    close: bool,
) {
    if poly.len() < 2 {
        return;
    }
    let mut r = rng(seed);
    let mut pts: Vec<(f32, f32)> = poly
        .iter()
        .map(|(x, y)| (*x as f32 + (r() - 0.5) * amp, *y as f32 + (r() - 0.5) * amp))
        .collect();
    if close {
        pts.push(pts[0]);
    }
    for win in pts.windows(2) {
        draw_line(
            img,
            win[0].0,
            win[0].1,
            win[1].0,
            win[1].1,
            color,
            0.95,
            width,
        );
    }
}

fn scribble(
    img: &mut RgbImage,
    poly: &[(i32, i32)],
    colors: &IllustrationPaletteColors,
    seed: u32,
) {
    let mut r = rng(seed);
    let cx = poly.iter().map(|p| p.0).sum::<i32>() as f32 / poly.len().max(1) as f32;
    let cy = poly.iter().map(|p| p.1).sum::<i32>() as f32 / poly.len().max(1) as f32;
    for accent in colors.accents.iter().take(3) {
        let ox = (r() - 0.5) * 10.0;
        let oy = (r() - 0.5) * 10.0;
        let shifted: Vec<(i32, i32)> = poly
            .iter()
            .map(|(x, y)| {
                let dx = *x as f32 - cx;
                let dy = *y as f32 - cy;
                let s = 1.0 + (r() - 0.5) * 0.08;
                (
                    (cx + dx * s + ox).round() as i32,
                    (cy + dy * s + oy).round() as i32,
                )
            })
            .collect();
        wob_outline(img, &shifted, parse_hex(accent), 1.5, 1.1, seed, true);
    }
}

fn construction(img: &mut RgbImage, cx: f32, cy: f32, radius: f32, seed: u32, color: Rgb<u8>) {
    let mut r = rng(seed);
    for _ in 0..3 {
        let a = r() * std::f32::consts::PI;
        let line_len = radius * (1.3 + r() * 1.2);
        draw_line(
            img,
            cx - a.cos() * line_len,
            cy - a.sin() * line_len,
            cx + a.cos() * line_len,
            cy + a.sin() * line_len,
            color,
            0.55,
            1.0,
        );
    }
    // circle approx
    let n = 48;
    for i in 0..n {
        let a0 = i as f32 / n as f32 * TAU;
        let a1 = (i + 1) as f32 / n as f32 * TAU;
        let rr = radius * 1.12;
        draw_line(
            img,
            cx + a0.cos() * rr,
            cy + a0.sin() * rr,
            cx + a1.cos() * rr,
            cy + a1.sin() * rr,
            color,
            0.45,
            0.9,
        );
    }
}

fn draw_line_clipped(
    img: &mut RgbImage,
    poly: &[(i32, i32)],
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: Rgb<u8>,
    al: f32,
) {
    let steps = ((x1 - x0).hypot(y1 - y0) as i32).max(1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = (x0 + (x1 - x0) * t).round() as i32;
        let y = (y0 + (y1 - y0) * t).round() as i32;
        if point_in_poly(x, y, poly) {
            put(img, x, y, color, al);
        }
    }
}

fn draw_line(
    img: &mut RgbImage,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: Rgb<u8>,
    al: f32,
    width: f32,
) {
    let steps = ((x1 - x0).hypot(y1 - y0) as i32).max(1);
    let half = (width * 0.5).max(0.5) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = (x0 + (x1 - x0) * t).round() as i32;
        let y = (y0 + (y1 - y0) * t).round() as i32;
        for dy in -half..=half {
            for dx in -half..=half {
                if dx * dx + dy * dy <= half * half + 1 {
                    put(img, x + dx, y + dy, color, al);
                }
            }
        }
    }
}

fn fill_box(img: &mut RgbImage, x: i32, y: i32, w: i32, h: i32, color: Rgb<u8>, al: f32) {
    for yy in y..(y + h) {
        for xx in x..(x + w) {
            put(img, xx, yy, color, al);
        }
    }
}

fn encode_png(img: &RgbImage) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Sidecar JSON for round-trip.
pub fn export_sidecar_json(doc: &IllustrationDoc) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(doc).map_err(|e| e.to_string())
}

/// Render one animation frame with pose/camera overrides.
pub fn export_frame_png(
    base: &IllustrationDoc,
    pose: &aos_proto::IllustrationPose,
    camera: &aos_proto::IllustrationCamera,
    mode: IllustrationRenderMode,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let mut doc = base.clone();
    if let Some(spec) = doc.spec.as_mut() {
        spec.pose = pose.clone();
        spec.camera = camera.clone();
        spec.mode = mode;
        // Apply tilt/walk as slight camera nudge for visible motion without rewriting paths.
        spec.camera.rot += pose.tilt * 0.15;
        spec.camera.x += pose.walk * 0.02;
        spec.camera.y += pose.twitch * 0.01;
        spec.camera.zoom *= 1.0 + pose.flap * 0.05;
    }
    export_png(&doc, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{
        IllustrationBrief, IllustrationLook, IllustrationPaletteId, IllustrationPart,
        IllustrationPartGeometry, IllustrationSpec,
    };

    #[test]
    fn raster_empty_paper() {
        let doc = IllustrationDoc {
            brief: IllustrationBrief {
                subject: "test".into(),
                look: IllustrationLook::Ink,
                palette: IllustrationPaletteId::PaperInk,
                ..Default::default()
            },
            ..Default::default()
        };
        let png = export_png(&doc, 128, 128).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    #[test]
    fn raster_with_ellipse() {
        let mut doc = IllustrationDoc {
            brief: IllustrationBrief {
                subject: "blob".into(),
                look: IllustrationLook::Ink,
                palette: IllustrationPaletteId::PaperInk,
                anchor: "blob".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        doc.spec = Some(IllustrationSpec {
            parts: vec![IllustrationPart {
                id: "body".into(),
                role: "body".into(),
                fill_index: 0,
                fill: true,
                outline: true,
                seed: 3,
                geometry: IllustrationPartGeometry::Ellipse {
                    x: 0.25,
                    y: 0.25,
                    w: 0.5,
                    h: 0.5,
                    rotation: 0.1,
                },
            }],
            show_construction: true,
            ..Default::default()
        });
        let png = export_png(&doc, 256, 256).unwrap();
        assert!(png.len() > 500);
        let sheet = export_sheet_png(&doc, 200).unwrap();
        assert!(sheet.starts_with(&[0x89, b'P', b'N', b'G']));
    }
}
