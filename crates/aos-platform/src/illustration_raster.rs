//! Illustration finish raster — paper, wob outlines, hatch / dot screens (skill-inspired).

use aos_proto::{
    IllustrationDoc, IllustrationFinish, IllustrationLook, IllustrationPaletteColors,
    IllustrationPart, IllustrationPartGeometry, IllustrationRenderMode, IllustrationSpec,
};
use image::{GrayImage, ImageBuffer, Luma, Rgb, RgbImage};
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
    rasterize(doc, width, height, None)
}

/// Same still, drawn up to `progress` (0..1). Subject parts appear in order:
/// outline first, fill once the stroke has closed. `progress >= 1` matches `export_png`.
pub fn export_png_progress(
    doc: &IllustrationDoc,
    width: u32,
    height: u32,
    progress: f32,
) -> Result<Vec<u8>, String> {
    let progress = progress.clamp(0.0, 1.0);
    rasterize(
        doc,
        width,
        height,
        if progress >= 0.999 {
            None
        } else {
            Some(progress)
        },
    )
}

/// How long the panel should take to play the current subject, in seconds.
pub fn draw_in_seconds(spec: &IllustrationSpec) -> f32 {
    let n = spec
        .parts
        .iter()
        .filter(|p| !part_is_background(p))
        .count()
        .max(1);
    (n as f32 * 0.5).clamp(1.8, 8.0)
}

fn rasterize(
    doc: &IllustrationDoc,
    width: u32,
    height: u32,
    progress: Option<f32>,
) -> Result<Vec<u8>, String> {
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
        paint_spec(&mut img, spec, &colors, night, progress);
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
    paint_spec(&mut tile, &scaled, colors, false, None);
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
    progress: Option<f32>,
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

    // Construction guides only in blueprint / explicit construction mode — never neon fringe on stills.
    if blueprint && spec.show_construction {
        construction(img, cam.x * w, cam.y * h, w.min(h) * 0.28, 11, parse_hex(&colors.guide));
    }

    // Skill riso path: separate plates per ink, then printPlate (multiply halftone).
    // Progressive playback uses the stroke loop so each part can draw in.
    if progress.is_none() && !blueprint && finish == IllustrationFinish::Riso && !colors.inks.is_empty() {
        paint_riso_plates(img, spec, colors, cam);
        for part in &spec.parts {
            if part_is_background(part) {
                continue;
            }
            let poly = part_poly(part, w, h, cam);
            if poly.len() < 3 {
                continue;
            }
            if part.outline {
                let eye = part.id.contains("eye") || part.role == "eye";
                let authored = matches!(part.geometry, IllustrationPartGeometry::Path { .. });
                draw_wob_outline(
                    img,
                    &poly,
                    outline_color,
                    IllustrationFinish::Riso,
                    part.seed.wrapping_add(7),
                    if eye {
                        0.35
                    } else if authored {
                        0.28
                    } else {
                        1.0
                    },
                    1.0,
                );
            }
            if spec.scribble_part.as_deref() == Some(part.id.as_str()) {
                scribble_ink(img, &poly, colors, part.seed.wrapping_add(99));
            }
        }
        let _ = (spec.pose.twitch, spec.pose.tilt);
        return;
    }

    let subject_n = spec
        .parts
        .iter()
        .filter(|p| !part_is_background(p))
        .count()
        .max(1);
    let reveal = progress.map(|p| {
        let x = p.clamp(0.0, 0.9999) * subject_n as f32;
        (x.floor() as usize, x - x.floor())
    });
    let mut subject_i = 0usize;

    for part in &spec.parts {
        let poly = part_poly(part, w, h, cam);
        let bg = part_is_background(part);
        if poly.len() < 3 {
            if !bg {
                subject_i += 1;
            }
            continue;
        }
        let mut stroke_frac = 1.0f32;
        let mut allow_fill = true;
        if !bg {
            if let Some((done, stroke)) = reveal {
                if subject_i > done {
                    break;
                }
                if subject_i == done {
                    stroke_frac = stroke;
                    allow_fill = stroke > 0.78;
                }
            }
            subject_i += 1;
        }
        let fill_c = if blueprint {
            parse_hex(&colors.night)
        } else if bg {
            // Full-frame bg uses paper wash, not a loud fill[0] (skill: paper first).
            mix(parse_hex(&colors.paper), parse_hex(&colors.fills[0]), 0.22)
        } else if part.id.contains("eye") {
            parse_hex(&colors.light)
        } else if part_is_furniture(part) {
            mix(
                parse_hex(&colors.paper),
                parse_hex(colors.fills.get(idx_fill(part, colors)).unwrap_or(&colors.fills[0])),
                0.42,
            )
        } else if part.id.contains("snout") {
            mix(
                parse_hex(&colors.light),
                parse_hex(colors.fills.get(idx_fill(part, colors)).unwrap_or(&colors.fills[0])),
                0.35,
            )
        } else {
            mix(
                parse_hex(colors.fills.get(idx_fill(part, colors)).unwrap_or(&colors.fills[0])),
                parse_hex(&colors.shade),
                0.16,
            )
        };
        if !blueprint && allow_fill && (part.id == "body" || part.id.contains("paw")) {
            contact_shadow(img, &poly, parse_hex(&colors.shade));
        }
        if part.fill && !blueprint && allow_fill {
            fill_poly(img, &poly, fill_c, 1.0);
            if bg {
                grain(img, Some(&poly), 220, shade(fill_c, 0.45), 0.12, part.seed);
            } else if !part.id.contains("eye") {
                let furniture = part_is_furniture(part);
                let angle = if furniture {
                    1.35
                } else if part.id.contains("ear") {
                    1.15
                } else {
                    0.4
                };
                let density = if furniture { 0.42 } else { 0.85 };
                surface_ex(
                    img,
                    &poly,
                    finish,
                    parse_hex(&colors.shade),
                    part.seed,
                    angle,
                    density,
                );
                form_shade(
                    img,
                    &poly,
                    parse_hex(&colors.shade),
                    if furniture { 0.4 } else { 1.0 },
                );
            }
        }
        if !bg && (part.outline || blueprint) {
            if blueprint && stroke_frac >= 0.995 {
                wob_outline(img, &poly, outline_color, 1.2, 2.6, part.seed.wrapping_add(7), true);
            } else {
                let eye = part.id.contains("eye") || part.role == "eye";
                let authored = matches!(
                    part.geometry,
                    IllustrationPartGeometry::Path { .. }
                );
                draw_wob_outline(
                    img,
                    &poly,
                    outline_color,
                    finish,
                    part.seed.wrapping_add(7),
                    if eye {
                        0.35
                    } else if authored {
                        0.28
                    } else {
                        1.0
                    },
                    stroke_frac,
                );
            }
        }
        if !bg && allow_fill && part.fill && part.id.contains("eye") && !blueprint {
            ink_pupil(img, &poly, outline_color);
        }
        if !bg && allow_fill && spec.scribble_part.as_deref() == Some(part.id.as_str()) && !blueprint {
            scribble_ink(img, &poly, colors, part.seed.wrapping_add(99));
        }
    }

    let _ = (spec.pose.twitch, spec.pose.tilt);
}

fn part_is_furniture(part: &IllustrationPart) -> bool {
    let id = part.id.to_ascii_lowercase();
    let role = part.role.to_ascii_lowercase();
    role == "furniture"
        || id.contains("sofa")
        || id.contains("couch")
        || id.contains("canap")
        || id.contains("cushion")
        || id.contains("coussin")
}

fn idx_fill(part: &IllustrationPart, colors: &IllustrationPaletteColors) -> usize {
    let n = colors.fills.len().max(1);
    part.fill_index as usize % n
}

fn part_is_background(part: &IllustrationPart) -> bool {
    let id = part.id.to_ascii_lowercase();
    let role = part.role.to_ascii_lowercase();
    role == "background"
        || role == "bg"
        || id.starts_with("bg")
        || id.contains("sky")
        || id.contains("ciel")
        || id.contains("sand")
        || id.contains("sable")
        || id.contains("sea")
        || id.contains("mer")
        || id.contains("ground")
        || id.contains("room")
}

/// Skill-style colour separations: black coverage on white plates → rotated dot screens.
fn paint_riso_plates(
    img: &mut RgbImage,
    spec: &IllustrationSpec,
    colors: &IllustrationPaletteColors,
    cam: &aos_proto::IllustrationCamera,
) {
    let w = img.width();
    let h = img.height();
    let n_inks = colors.inks.len().min(4).max(1);
    let mut plates: Vec<GrayImage> = (0..n_inks)
        .map(|_| ImageBuffer::from_pixel(w, h, Luma([255u8])))
        .collect();

    for part in &spec.parts {
        if !part.fill {
            continue;
        }
        if part_is_background(part) {
            let (_, _, bw, bh) = match &part.geometry {
                IllustrationPartGeometry::Ellipse { w, h, .. }
                | IllustrationPartGeometry::Rect { w, h, .. } => (0.0, 0.0, *w, *h),
                IllustrationPartGeometry::Path { .. } => (0.0, 0.0, 1.0, 1.0),
            };
            // Full-bleed bg stays as paper stock; don't flood a whole plate.
            if bw * bh > 0.85 {
                continue;
            }
        }
        let poly = part_poly(part, w as f32, h as f32, cam);
        if poly.len() < 3 {
            continue;
        }
        let plate_i = part.fill_index as usize % n_inks;
        fill_poly_gray(&mut plates[plate_i], &poly, 0);
    }

    let angles = [0.26f32, 1.31, 0.0, 0.78];
    for (k, plate) in plates.iter().enumerate() {
        let ink = parse_hex(&colors.inks[k]);
        print_plate(
            img,
            plate,
            ink,
            7.0,
            angles[k % angles.len()],
            0.2,
            30 + k as u32,
            1.0,
            0.78,
            0.95,
        );
    }
}

fn fill_poly_gray(img: &mut GrayImage, poly: &[(i32, i32)], value: u8) {
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
    if min_x > max_x || min_y > max_y {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if point_in_poly(x, y, poly) {
                img.put_pixel(x as u32, y as u32, Luma([value]));
            }
        }
    }
}

/// Downsample coverage + rotated halftone dots in ink (multiply onto paper).
fn print_plate(
    img: &mut RgbImage,
    coverage: &GrayImage,
    ink: Rgb<u8>,
    cell: f32,
    angle: f32,
    jitter: f32,
    seed: u32,
    gain: f32,
    max_cov: f32,
    alpha: f32,
) {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let sw = ((w as f32) / cell).ceil().max(1.0) as u32;
    let sh = ((h as f32) / cell).ceil().max(1.0) as u32;
    let mut cov_small: GrayImage = ImageBuffer::new(sw, sh);
    for y in 0..sh {
        for x in 0..sw {
            let sx = ((x as f32 + 0.5) * cell).min((w - 1) as f32) as u32;
            let sy = ((y as f32 + 0.5) * cell).min((h - 1) as f32) as u32;
            let Luma([v]) = *coverage.get_pixel(sx, sy);
            cov_small.put_pixel(x, y, Luma([v]));
        }
    }

    let mut r = rng(seed);
    let cx = w as f32 * 0.5;
    let cy = h as f32 * 0.5;
    let radius = ((w * w + h * h) as f32).sqrt() * 0.5;
    let ca = angle.cos();
    let sa = angle.sin();
    let mut v = -radius;
    while v <= radius {
        let mut u = -radius;
        while u <= radius {
            let x = cx + ca * u - sa * v + (r() - 0.5) * jitter * cell;
            let y = cy + sa * u + ca * v + (r() - 0.5) * jitter * cell;
            if x >= 0.0 && y >= 0.0 && x < w as f32 && y < h as f32 {
                let ix = (x / cell).floor().clamp(0.0, (sw - 1) as f32) as u32;
                let iy = (y / cell).floor().clamp(0.0, (sh - 1) as f32) as u32;
                let Luma([lum]) = *cov_small.get_pixel(ix, iy);
                let cov = ((1.0 - lum as f32 / 255.0) * gain).clamp(0.0, max_cov);
                if cov >= 0.03 {
                    let rad = (cell * 0.62 * cov.sqrt()).round().max(1.0) as i32;
                    // Multiply blend via put with ink over paper.
                    fill_circle(img, x.round() as i32, y.round() as i32, rad, ink, alpha * cov);
                }
            }
            u += cell;
        }
        v += cell;
    }
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
            let (x, y, bw, bh) = unitize_bbox(*x, *y, *bw, *bh);
            let cx = x + bw * 0.5;
            let cy = y + bh * 0.5;
            let rx = bw * 0.5;
            let ry = bh * 0.5;
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
            let (x, y, bw, bh) = unitize_bbox(*x, *y, *bw, *bh);
            let cx = x + bw * 0.5;
            let cy = y + bh * 0.5;
            let corners = [
                (x, y),
                (x + bw, y),
                (x + bw, y + bh),
                (x, y + bh),
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
            let scale = path_unit_scale(points);
            points
                .iter()
                .map(|p| map(p.x / scale, p.y / scale))
                .collect()
        }
    }
}

/// Agents often emit 0..100 (%) or ~1024px; raster expects 0..1.
fn unit_coord_divisor(samples: &[f32]) -> f32 {
    let max_abs = samples
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0f32, f32::max);
    if !max_abs.is_finite() || max_abs <= 1.5 {
        1.0
    } else if max_abs <= 100.5 {
        100.0
    } else if max_abs <= 2048.0 {
        1024.0
    } else {
        max_abs
    }
}

fn unitize_bbox(x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
    let div = unit_coord_divisor(&[x, y, w, h, x + w, y + h]);
    (x / div, y / div, (w / div).max(0.01), (h / div).max(0.01))
}

fn path_unit_scale(points: &[aos_proto::CanvasPoint]) -> f32 {
    let mut samples = Vec::with_capacity(points.len() * 2);
    for p in points {
        samples.push(p.x);
        samples.push(p.y);
    }
    unit_coord_divisor(&samples)
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

fn surface_ex(
    img: &mut RgbImage,
    poly: &[(i32, i32)],
    finish: IllustrationFinish,
    shade_c: Rgb<u8>,
    seed: u32,
    angle: f32,
    density: f32,
) {
    let density = density.clamp(0.2, 1.4);
    match finish {
        IllustrationFinish::Ink => {
            hatch(img, poly, angle, 4.5, 9.0, shade_c, 0.35 * density, seed);
            grain(img, Some(poly), 140, shade_c, 0.3, seed.wrapping_add(1));
        }
        IllustrationFinish::Pencil => {
            let gap = 9.0 / density;
            hatch(
                img,
                poly,
                angle,
                gap,
                16.0,
                shade_c,
                0.2 * density,
                seed,
            );
            grain(img, Some(poly), 50, shade_c, 0.16, seed.wrapping_add(1));
        }
        IllustrationFinish::Riso => {
            dot_screen(img, poly, 7.0, shade_c, 0.55 * density, 0.26, 0.35, seed);
        }
        IllustrationFinish::Screen => {
            dot_screen(img, poly, 6.0, shade_c, 0.5, 0.0, 0.06, seed);
        }
        IllustrationFinish::Flat => {}
    }
}

/// Darker toward the bottom of a part so a flat fill reads as volume.
fn form_shade(img: &mut RgbImage, poly: &[(i32, i32)], shade: Rgb<u8>, weight: f32) {
    if poly.len() < 3 || weight < 0.01 {
        return;
    }
    let min_x = poly.iter().map(|p| p.0).min().unwrap_or(0);
    let max_x = poly.iter().map(|p| p.0).max().unwrap_or(0);
    let min_y = poly.iter().map(|p| p.1).min().unwrap_or(0);
    let max_y = poly.iter().map(|p| p.1).max().unwrap_or(0);
    let span = (max_y - min_y).max(1) as f32;
    for y in min_y..=max_y {
        let t = (y - min_y) as f32 / span;
        let a = ((t - 0.45) * 0.38).clamp(0.0, 0.22) * weight;
        if a < 0.02 {
            continue;
        }
        for x in min_x..=max_x {
            if point_in_poly(x, y, poly) {
                put(img, x, y, shade, a);
            }
        }
    }
}

fn contact_shadow(img: &mut RgbImage, poly: &[(i32, i32)], shade: Rgb<u8>) {
    if poly.is_empty() {
        return;
    }
    let min_x = poly.iter().map(|p| p.0).min().unwrap_or(0);
    let max_x = poly.iter().map(|p| p.0).max().unwrap_or(0);
    let max_y = poly.iter().map(|p| p.1).max().unwrap_or(0);
    let cx = (min_x + max_x) / 2;
    let rx = ((max_x - min_x) / 2).max(6);
    let ry = (rx / 5).max(3);
    let cy = max_y + ry / 2;
    for y in (cy - ry)..=(cy + ry) {
        for x in (cx - rx)..=(cx + rx) {
            let nx = (x - cx) as f32 / rx as f32;
            let ny = (y - cy) as f32 / ry as f32;
            let d = nx * nx + ny * ny;
            if d <= 1.0 {
                put(img, x, y, shade, (1.0 - d) * 0.32);
            }
        }
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

/// Skill rule 3: fill stays the geometric part; the stroke is a separately
/// resampled polyline pushed outward so it does not ride the fill edge.
fn draw_wob_outline(
    img: &mut RgbImage,
    poly: &[(i32, i32)],
    color: Rgb<u8>,
    finish: IllustrationFinish,
    seed: u32,
    amp_scale: f32,
    stroke_frac: f32,
) {
    if poly.len() < 3 || stroke_frac <= 0.001 {
        return;
    }
    let span = poly_span(poly);
    let (amp, width, passes) = match finish {
        IllustrationFinish::Pencil => (span * 0.06, 1.65, 1u32),
        IllustrationFinish::Ink | IllustrationFinish::Screen => (span * 0.12, 2.15, 2),
        IllustrationFinish::Riso => (span * 0.11, 2.5, 3),
        IllustrationFinish::Flat => (span * 0.04, 1.15, 1),
    };
    let amp = (amp * amp_scale).clamp(2.2, 28.0);
    let close = stroke_frac >= 0.995;
    for k in 0..passes {
        let pts = wob_contour(poly, amp * (1.0 + k as f32 * 0.12), seed.wrapping_add(k * 11));
        let pts = if close {
            pts
        } else {
            clip_ring(&pts, stroke_frac)
        };
        if pts.len() < 2 {
            continue;
        }
        let al = if k == 0 { 0.92 } else { 0.34 };
        let w = if k == 0 { width } else { width * 0.65 };
        stroke_polyline(img, &pts, color, al, w, close);
    }
}

/// Walk a closed ring until `frac` of its perimeter, leaving the stroke open.
fn clip_ring(pts: &[(f32, f32)], frac: f32) -> Vec<(f32, f32)> {
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let mut lens = Vec::with_capacity(n + 1);
    lens.push(0.0f32);
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        lens.push(lens[i] + (b.0 - a.0).hypot(b.1 - a.1));
    }
    let total = *lens.last().unwrap_or(&0.0);
    if total < 1.0 {
        return pts.to_vec();
    }
    let budget = total * frac.clamp(0.0, 1.0);
    let mut out = vec![pts[0]];
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let l1 = lens[i + 1];
        if l1 <= budget + 0.01 {
            out.push(b);
            continue;
        }
        let seg = (l1 - lens[i]).max(0.001);
        let t = ((budget - lens[i]) / seg).clamp(0.0, 1.0);
        out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        break;
    }
    out
}

fn ink_pupil(img: &mut RgbImage, poly: &[(i32, i32)], ink: Rgb<u8>) {
    if poly.is_empty() {
        return;
    }
    let cx = poly.iter().map(|p| p.0).sum::<i32>() / poly.len() as i32;
    let cy = poly.iter().map(|p| p.1).sum::<i32>() / poly.len() as i32;
    let min_x = poly.iter().map(|p| p.0).min().unwrap_or(cx);
    let max_x = poly.iter().map(|p| p.0).max().unwrap_or(cx);
    let r = ((max_x - min_x) / 5).max(2);
    fill_circle(img, cx, cy + r / 3, r, ink, 0.95);
}

fn poly_span(poly: &[(i32, i32)]) -> f32 {
    let min_x = poly.iter().map(|p| p.0).min().unwrap_or(0);
    let max_x = poly.iter().map(|p| p.0).max().unwrap_or(0);
    let min_y = poly.iter().map(|p| p.1).min().unwrap_or(0);
    let max_y = poly.iter().map(|p| p.1).max().unwrap_or(0);
    ((max_x - min_x).min(max_y - min_y) as f32).max(8.0)
}

/// Even samples around a closed polygon, offset along the outward radial
/// with a low-frequency wave so a circle does not stay a circle.
fn wob_contour(poly: &[(i32, i32)], amp: f32, seed: u32) -> Vec<(f32, f32)> {
    let n = poly.len().clamp(36, 84);
    let sampled = resample_closed(poly, n);
    if sampled.len() < 3 {
        return sampled;
    }
    let cx = sampled.iter().map(|p| p.0).sum::<f32>() / sampled.len() as f32;
    let cy = sampled.iter().map(|p| p.1).sum::<f32>() / sampled.len() as f32;
    let mut r = rng(seed);
    let phase = r() * TAU;
    let lobes = 2.0 + r() * 2.0;
    sampled
        .into_iter()
        .enumerate()
        .map(|(i, (x, y))| {
            let dx = x - cx;
            let dy = y - cy;
            let len = dx.hypot(dy).max(1.0);
            let nx = dx / len;
            let ny = dy / len;
            let t = i as f32 / n as f32;
            let wave = (t * TAU * lobes + phase).sin();
            let jitter = (r() - 0.5) * 2.0;
            let off = amp * (0.7 + 0.4 * wave) + amp * 0.22 * jitter;
            let tang = amp * 0.16 * (r() - 0.5) * 2.0;
            (x + nx * off - ny * tang, y + ny * off + nx * tang)
        })
        .collect()
}

fn resample_closed(poly: &[(i32, i32)], n: usize) -> Vec<(f32, f32)> {
    if n == 0 || poly.is_empty() {
        return Vec::new();
    }
    let mut pts: Vec<(f32, f32)> = poly.iter().map(|p| (p.0 as f32, p.1 as f32)).collect();
    if let (Some(a), Some(b)) = (pts.first().copied(), pts.last().copied()) {
        if (a.0 - b.0).abs() > 0.5 || (a.1 - b.1).abs() > 0.5 {
            pts.push(a);
        }
    }
    if pts.len() < 2 {
        return pts;
    }
    let mut lens = vec![0.0f32];
    for w in pts.windows(2) {
        let d = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        lens.push(lens.last().copied().unwrap_or(0.0) + d);
    }
    let total = *lens.last().unwrap_or(&0.0);
    if total < 1.0 {
        return pts;
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let target = total * (i as f32 / n as f32);
        let seg = lens
            .iter()
            .position(|l| *l >= target)
            .unwrap_or(lens.len() - 1)
            .max(1);
        let l0 = lens[seg - 1];
        let l1 = lens[seg];
        let t = if l1 > l0 {
            (target - l0) / (l1 - l0)
        } else {
            0.0
        };
        let a = pts[seg - 1];
        let b = pts[seg.min(pts.len() - 1)];
        out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
    }
    out
}

fn stroke_polyline(
    img: &mut RgbImage,
    pts: &[(f32, f32)],
    color: Rgb<u8>,
    al: f32,
    width: f32,
    close: bool,
) {
    if pts.len() < 2 {
        return;
    }
    let last = if close { pts.len() } else { pts.len() - 1 };
    for i in 0..last {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        draw_line(img, a.0, a.1, b.0, b.1, color, al, width);
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

fn scribble_ink(
    img: &mut RgbImage,
    poly: &[(i32, i32)],
    colors: &IllustrationPaletteColors,
    seed: u32,
) {
    // One quiet ink scribble on the main mass — never neon accents (skill accents are sparks, not outlines).
    let mut r = rng(seed);
    let cx = poly.iter().map(|p| p.0).sum::<i32>() as f32 / poly.len().max(1) as f32;
    let cy = poly.iter().map(|p| p.1).sum::<i32>() as f32 / poly.len().max(1) as f32;
    let ox = (r() - 0.5) * 8.0;
    let oy = (r() - 0.5) * 8.0;
    let shifted: Vec<(i32, i32)> = poly
        .iter()
        .map(|(x, y)| {
            let dx = *x as f32 - cx;
            let dy = *y as f32 - cy;
            let s = 1.0 + (r() - 0.5) * 0.06;
            (
                (cx + dx * s + ox).round() as i32,
                (cy + dy * s + oy).round() as i32,
            )
        })
        .collect();
    wob_outline(
        img,
        &shifted,
        parse_hex(&colors.ink),
        2.2,
        1.2,
        seed,
        true,
    );
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
    let half = width * 0.5;
    let radius = half.ceil() as i32 + 1;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = x0 + (x1 - x0) * t;
        let y = y0 + (y1 - y0) * t;
        let ix = x.round() as i32;
        let iy = y.round() as i32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let d = ((ix + dx) as f32 - x).hypot((iy + dy) as f32 - y);
                let cover = (half + 0.65 - d).clamp(0.0, 1.0);
                if cover > 0.04 {
                    put(img, ix + dx, iy + dy, color, al * cover);
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

    #[test]
    fn raster_riso_print_plates_with_puppet() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "chat sur un coussin".into(),
                look: IllustrationLook::Ink,
                palette: IllustrationPaletteId::RisoPop,
                ..Default::default()
            },
            parts: vec![IllustrationPart {
                id: "blob".into(),
                role: "main".into(),
                fill_index: 1,
                fill: true,
                outline: true,
                seed: 1,
                geometry: IllustrationPartGeometry::Ellipse {
                    x: 0.3,
                    y: 0.3,
                    w: 0.4,
                    h: 0.4,
                    rotation: 0.0,
                },
            }],
            ..Default::default()
        };
        aos_proto::enrich_illustration_puppet(&mut spec);
        let doc = IllustrationDoc {
            brief: spec.brief.clone(),
            spec: Some(spec),
            ..Default::default()
        };
        let png = export_png(&doc, 256, 256).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.len() > 800);
    }

    #[test]
    fn wob_contour_breaks_perfect_circle() {
        let n = 48;
        let poly: Vec<(i32, i32)> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * TAU;
                (
                    (128.0 + a.cos() * 60.0).round() as i32,
                    (128.0 + a.sin() * 60.0).round() as i32,
                )
            })
            .collect();
        let wob = wob_contour(&poly, 8.0, 7);
        let radii: Vec<f32> = wob
            .iter()
            .map(|(x, y)| (x - 128.0).hypot(y - 128.0))
            .collect();
        let min = radii.iter().copied().fold(f32::MAX, f32::min);
        let max = radii.iter().copied().fold(0.0f32, f32::max);
        assert!(
            max - min > 4.0,
            "wob should vary radius, got {min:.1}..{max:.1}"
        );
    }

    #[test]
    fn progress_draws_less_than_the_still() {
        let mut doc = IllustrationDoc {
            brief: IllustrationBrief {
                subject: "blob".into(),
                look: IllustrationLook::Pencil,
                palette: IllustrationPaletteId::PencilMinimal,
                anchor: "blob".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        doc.spec = Some(IllustrationSpec {
            parts: vec![
                IllustrationPart {
                    id: "body".into(),
                    role: "body".into(),
                    fill_index: 0,
                    fill: true,
                    outline: true,
                    seed: 3,
                    geometry: IllustrationPartGeometry::Ellipse {
                        x: 0.2,
                        y: 0.25,
                        w: 0.45,
                        h: 0.35,
                        rotation: 0.0,
                    },
                },
                IllustrationPart {
                    id: "head".into(),
                    role: "head".into(),
                    fill_index: 1,
                    fill: true,
                    outline: true,
                    seed: 9,
                    geometry: IllustrationPartGeometry::Ellipse {
                        x: 0.48,
                        y: 0.18,
                        w: 0.28,
                        h: 0.24,
                        rotation: 0.0,
                    },
                },
            ],
            ..Default::default()
        });
        let early = export_png_progress(&doc, 160, 160, 0.12).unwrap();
        let mid = export_png_progress(&doc, 160, 160, 0.55).unwrap();
        let full = export_png(&doc, 160, 160).unwrap();
        let done = export_png_progress(&doc, 160, 160, 1.0).unwrap();
        assert_ne!(early, mid);
        assert_ne!(mid, full);
        assert_eq!(done, full);
        let open = clip_ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], 0.25);
        assert!(open.len() >= 2);
        assert!(open.len() < 5);
    }
}
