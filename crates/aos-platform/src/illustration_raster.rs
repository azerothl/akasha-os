//! Illustration finish raster — paper, wob outlines, hatch / dot screens (skill-inspired).

use aos_proto::{
    IllustrationArchetype, IllustrationConstructionPhase, IllustrationDoc, IllustrationFinish, IllustrationLook,
    IllustrationPaletteColors, IllustrationPart, IllustrationPartGeometry, IllustrationRenderMode,
    IllustrationSpec,
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
    use aos_proto::IllustrationConstructionPhase as Phase;
    let n = match spec.construction_phase {
        Phase::Skeleton => spec
            .skeleton
            .len()
            .max(spec.construction.action_line_points.len().saturating_sub(1))
            .max(1),
        Phase::Volumes => spec.volumes.len().max(1),
        Phase::Contours => spec.contours.len().max(1),
        Phase::Details => spec.details.len().max(1),
        Phase::Final => spec
            .parts
            .iter()
            .filter(|p| !part_is_background(p))
            .count()
            .max(1),
    };
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
    // The illustration is made from polygonal contours and hand-drawn strokes.
    // Painting directly at delivery resolution made every contour a stair-step,
    // especially at the 240 px review size. Render the drawing at 2x and reduce
    // it once at the end so the final image has real coverage instead of relying
    // on the wobble pass to hide aliasing.
    let render_scale = if w <= 2048 && h <= 2048 { 2 } else { 1 };
    let rw = w.saturating_mul(render_scale);
    let rh = h.saturating_mul(render_scale);
    let bytes = rasterize_at(doc, rw, rh, progress)?;
    if render_scale == 1 {
        return Ok(bytes);
    }
    let img = image::load_from_memory(&bytes)
        .map_err(|e| e.to_string())?
        .to_rgb8();
    let reduced = image::imageops::resize(
        &img,
        w,
        h,
        image::imageops::FilterType::Lanczos3,
    );
    encode_png(&reduced)
}

fn rasterize_at(
    doc: &IllustrationDoc,
    w: u32,
    h: u32,
    progress: Option<f32>,
) -> Result<Vec<u8>, String> {
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

    if look == IllustrationLook::Doodle {
        crate::illustration_doodle::paint_photo(
            &mut img,
            doc.brief.photo.trim(),
            &doc.brief.subject,
            &colors,
        );
    }

    if let Some(spec) = doc.spec.as_ref() {
        let resolved = spec.resolve_joint_bindings()?;
        let spec = &resolved;
        paint_spec(&mut img, spec, &colors, night, progress);
        if look == IllustrationLook::Doodle {
            crate::illustration_doodle::paint_glow(&mut img, spec, &colors);
        }
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

/// Character reference board generated from the construction model sheet.
/// Views are deliberately redrawn from the composed parts and then warped
/// only by a small view transform; this keeps the subject stable while making
/// the references useful for later key-drawing passes.
pub fn export_model_sheet_png(doc: &IllustrationDoc, cell: u32) -> Result<Vec<u8>, String> {
    let cell = cell.max(180);
    let cols = 3u32;
    let rows = 2u32;
    let colors = IllustrationPaletteColors::for_preset(doc.brief.palette);
    let mut img: RgbImage = ImageBuffer::from_pixel(cols * cell, rows * cell, parse_hex(&colors.paper));
    paper_stock(&mut img, &colors);
    let views = ["face", "profil", "trois_quarts", "dos", "expression_neutre", "pose_principale"];
    if let Some(spec) = doc.spec.as_ref() {
        for (i, view) in views.iter().enumerate() {
            let mut tile: RgbImage = ImageBuffer::from_pixel(cell, cell, parse_hex(&colors.paper));
            let view_spec = model_view_spec(spec, view);
            paint_spec(&mut tile, &view_spec, &colors, false, None);
            let ox = (i as u32 % cols) * cell;
            let oy = (i as u32 / cols) * cell;
            for y in 0..cell {
                for x in 0..cell {
                    let c = *tile.get_pixel(x, y);
                    put(&mut img, (ox + x) as i32, (oy + y) as i32, c, 1.0);
                }
            }
        }
    }
    encode_png(&img)
}

fn model_view_spec(spec: &IllustrationSpec, view: &str) -> IllustrationSpec {
    let mut out = spec.clone();
    if view == "pose_principale" {
        if let Some(drawing) = spec
            .key_drawings
            .iter()
            .find(|d| d.id == "land" || d.id == "settle")
        {
            out.parts = drawing.parts.clone();
        }
    }
    let scale_x = match view {
        "profil" | "dos" => 0.58,
        "trois_quarts" | "expression_neutre" => 0.84,
        _ => 1.0,
    };
    let mirror = view == "dos";
    let archetype = spec.construction.archetype;
    out.parts.retain(|part| {
        let id = part.id.to_ascii_lowercase();
        if view == "dos" {
            // A back reference keeps the outer construction but removes
            // features that only belong to the front-facing expression.
            return !(id.contains("eye")
                || id.contains("pipe")
                || id.contains("beard")
                || id.contains("mouth")
                || id.contains("snout")
                || id.contains("beak"));
        }
        if view == "profil" {
            // A profile has one visible eye/ear and no duplicate facial pair.
            if id.ends_with("eye_r") || id.ends_with("ear_r") {
                return false;
            }
            if archetype == IllustrationArchetype::Human && id == "arm" {
                return true;
            }
        }
        true
    });
    if view == "profil" && archetype == IllustrationArchetype::Quadruped {
        // Move the animal's face toward the leading end of the body instead
        // of squeezing the entire character into a generic narrow icon.
        for part in &mut out.parts {
            let id = part.id.to_ascii_lowercase();
            if id.contains("head") || id.contains("ear_l") || id.contains("eye_l") || id == "snout" {
                translate_part(part, -0.035, 0.0);
            }
        }
    }
    for part in &mut out.parts {
        match &mut part.geometry {
            IllustrationPartGeometry::Ellipse { x, w, .. }
            | IllustrationPartGeometry::Rect { x, w, .. } => {
                let cx = *x + *w * 0.5;
                let nx = if mirror { 1.0 - cx } else { 0.5 + (cx - 0.5) * scale_x };
                *w *= scale_x;
                *x = nx - *w * 0.5;
            }
            IllustrationPartGeometry::Path { points, .. } => {
                for p in points {
                    p.x = if mirror { 1.0 - p.x } else { 0.5 + (p.x - 0.5) * scale_x };
                }
            }
        }
    }
    out
}

fn translate_part(part: &mut IllustrationPart, dx: f32, dy: f32) {
    match &mut part.geometry {
        IllustrationPartGeometry::Ellipse { x, y, .. }
        | IllustrationPartGeometry::Rect { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        IllustrationPartGeometry::Path { points, .. } => {
            for p in points {
                p.x += dx;
                p.y += dy;
            }
        }
    }
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

    if !blueprint && spec.construction_phase != IllustrationConstructionPhase::Final {
        paint_construction_phase(img, spec, colors, progress);
        return;
    }

    if !blueprint
        && spec.construction_phase == IllustrationConstructionPhase::Final
        && spec.show_construction
        && !spec.skeleton.is_empty()
    {
        draw_skeleton_pose_alpha(
            img,
            spec,
            parse_hex(&colors.guide),
            parse_hex(&colors.blush),
            0.1,
            0.1,
            None,
        );
    }

    // A taxonomic pose with no dressed parts is still a drawing. Volumes and
    // the joint chain have to show, or the preview is only the paper stock.
    if spec.parts.is_empty() && (!spec.skeleton.is_empty() || !spec.volumes.is_empty()) {
        if !spec.volumes.is_empty() {
            let mut staged = spec.clone();
            staged.parts = spec.volumes.iter().map(volume_as_part).collect();
            paint_spec(img, &staged, colors, blueprint, progress);
        }
        if !spec.skeleton.is_empty() {
            draw_skeleton_pose(img, spec, parse_hex(&colors.ink), parse_hex(&colors.ink));
        }
        return;
    }

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
                let focal = part_is_focal(spec, part);
                draw_wob_outline(
                    img,
                    &poly,
                    outline_color,
                    IllustrationFinish::Riso,
                    part.seed.wrapping_add(7),
                    if focal {
                        0.48
                    } else if eye {
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
        let focal = part_is_focal(spec, part);
        let fill_c = if blueprint {
            parse_hex(&colors.night)
        } else if bg {
            // Full-frame bg uses paper wash, not a loud fill[0] (skill: paper first).
            mix(parse_hex(&colors.paper), parse_hex(&colors.fills[0]), 0.22)
        } else if part.id.contains("eye") {
            parse_hex(&colors.light)
        } else if part_mentions(part, &["canopy", "leaf", "foliage", "lawn"])
            || (part.id.contains("stem") && !part.id.contains("pipe"))
        {
            mix(
                parse_hex(colors.accents.first().unwrap_or(&colors.shade)),
                parse_hex(&colors.paper),
                0.28,
            )
        } else if part_mentions(part, &["flower", "bloom", "petal"]) {
            mix(
                parse_hex(colors.accents.get(1).unwrap_or(&colors.ink)),
                parse_hex(&colors.paper),
                0.22,
            )
        } else if part_mentions(part, &["trunk", "pipe", "bench", "hair", "beard"]) {
            mix(parse_hex(&colors.ink), parse_hex(&colors.paper), 0.62)
        } else if part.id.contains("head") || part.role == "head" {
            mix(parse_hex(&colors.blush), parse_hex(&colors.paper), 0.4)
        } else if part_is_furniture(part) {
            mix(
                parse_hex(&colors.paper),
                parse_hex(colors.fills.get(idx_fill(part, colors)).unwrap_or(&colors.fills[0])),
                0.42,
            )
        } else if part.id.contains("paper") || part.id.contains("page") || part.role == "prop" {
            mix(parse_hex(&colors.paper), parse_hex(&colors.light), 0.25)
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
        // The construction plan supplies a restrained value hierarchy: the
        // focal mass gets slightly more ink and less paper wash, while the
        // palette and hand-drawn finish remain unchanged.
        let fill_c = if !blueprint && focal {
            mix(fill_c, parse_hex(&colors.ink), 0.14)
        } else {
            fill_c
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
                let span = {
                    let min_x = poly.iter().map(|p| p.0).min().unwrap_or(0);
                    let max_x = poly.iter().map(|p| p.0).max().unwrap_or(0);
                    let min_y = poly.iter().map(|p| p.1).min().unwrap_or(0);
                    let max_y = poly.iter().map(|p| p.1).max().unwrap_or(0);
                    (max_x - min_x).max(max_y - min_y)
                };
                let angle = 0.7 + (part.seed % 5) as f32 * 0.28;
                let density = if focal {
                    0.52
                } else if furniture {
                    0.28
                } else if span < 40 {
                    0.15
                } else if part.id == "body" {
                    0.4
                } else {
                    0.32
                };
                if span > 18 {
                    surface_ex(
                        img,
                        &poly,
                        finish,
                        parse_hex(&colors.shade),
                        part.seed,
                        angle,
                        density,
                    );
                }
                form_shade(
                    img,
                    &poly,
                    parse_hex(&colors.shade),
                    if part.id == "body" {
                        0.7
                    } else if furniture {
                        0.3
                    } else {
                        0.15
                    },
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
                    if focal {
                        0.20
                    } else if eye {
                        0.22
                    } else if authored {
                        0.12
                    } else {
                        0.45
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

fn part_mentions(part: &IllustrationPart, needles: &[&str]) -> bool {
    let id = part.id.to_ascii_lowercase();
    needles.iter().any(|n| id.contains(n))
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
        IllustrationPartGeometry::Path { points, closed } => {
            let scale = path_unit_scale(points);
            let source: Vec<(f32, f32)> = points
                .iter()
                .map(|p| (p.x / scale, p.y / scale))
                .collect();
            let source = if *closed && source.len() >= 8 {
                smooth_contour(&source)
            } else {
                source
            };
            source.into_iter().map(|(x, y)| map(x, y)).collect()
        }
    }
}

fn draw_skeleton_pose(img: &mut RgbImage, spec: &IllustrationSpec, bone: Rgb<u8>, joint_color: Rgb<u8>) {
    draw_skeleton_pose_alpha(img, spec, bone, joint_color, 0.95, 0.95, None);
}

/// Draw the joint chain. `progress` reveals bones then joints in skeleton order
/// (None = fully drawn). Alphas let later passes keep the pose as a faint guide.
fn draw_skeleton_pose_alpha(
    img: &mut RgbImage,
    spec: &IllustrationSpec,
    bone: Rgb<u8>,
    joint_color: Rgb<u8>,
    bone_alpha: f32,
    joint_alpha: f32,
    progress: Option<f32>,
) {
    let w = img.width() as f32;
    let h = img.height() as f32;
    let n = spec.skeleton.len().max(1) as f32;
    let reveal = progress.map(|p| {
        let x = p.clamp(0.0, 0.9999) * n;
        (x.floor() as usize, (x - x.floor()).clamp(0.0, 1.0))
    });
    for (i, joint) in spec.skeleton.iter().enumerate() {
        let frac = match reveal {
            Some((done, _)) if i > done => continue,
            Some((done, stroke)) if i == done => stroke.max(0.04),
            _ => 1.0,
        };
        if let Some(parent) = joint
            .parent
            .as_deref()
            .and_then(|id| spec.skeleton.iter().find(|other| other.id == id))
        {
            let x0 = parent.x * w;
            let y0 = parent.y * h;
            let x1 = x0 + (joint.x * w - x0) * frac;
            let y1 = y0 + (joint.y * h - y0) * frac;
            draw_line(img, x0, y0, x1, y1, bone, bone_alpha, 2.4);
        }
    }
    for (i, joint) in spec.skeleton.iter().enumerate() {
        let show = match reveal {
            Some((done, _)) if i > done => false,
            Some((done, stroke)) if i == done => stroke > 0.55,
            _ => true,
        };
        if !show {
            continue;
        }
        fill_circle(
            img,
            (joint.x * w).round() as i32,
            (joint.y * h).round() as i32,
            (joint.radius.clamp(0.002, 0.008) * w).max(2.0).round() as i32,
            joint_color,
            joint_alpha,
        );
    }
}

fn paint_action_line_and_ovals(
    img: &mut RgbImage,
    spec: &IllustrationSpec,
    colors: &IllustrationPaletteColors,
    progress: Option<f32>,
) {
    let color = parse_hex(&colors.guide);
    let w = img.width() as f32;
    let h = img.height() as f32;
    let p = progress.unwrap_or(1.0).clamp(0.0, 1.0);
    let action_color = parse_hex(colors.accents.first().unwrap_or(&colors.ink));
    let segments = spec.construction.action_line_points.windows(2).collect::<Vec<_>>();
    if !segments.is_empty() {
        let n = segments.len() as f32;
        let action_span = if progress.is_some() { (p / 0.28).clamp(0.0, 1.0) } else { 1.0 };
        let reveal = action_span * n;
        let done = reveal.floor() as usize;
        let stroke = (reveal - done as f32).clamp(0.0, 1.0);
        for (i, pair) in segments.iter().enumerate() {
            if i > done {
                break;
            }
            let frac = if i == done { stroke.max(0.04) } else { 1.0 };
            let a = pair[0];
            let b = pair[1];
            let x0 = a.x * w;
            let y0 = a.y * h;
            let x1 = x0 + (b.x * w - x0) * frac;
            let y1 = y0 + (b.y * h - y0) * frac;
            draw_line(img, x0, y0, x1, y1, action_color, 0.9, 2.5);
        }
    }
    let ovals_ready = progress.map(|v| v >= 0.22).unwrap_or(true);
    if ovals_ready {
        if let Some(oval) = &spec.construction.chest_oval {
            draw_construction_oval(img, oval, w, h, color);
        }
        if progress.map(|v| v >= 0.28).unwrap_or(true) {
            if let Some(oval) = &spec.construction.pelvis_oval {
                draw_construction_oval(img, oval, w, h, color);
            }
        }
    }
}

/// Lightweight outlines so later passes keep earlier construction visible.
fn paint_volume_underlay(
    img: &mut RgbImage,
    spec: &IllustrationSpec,
    colors: &IllustrationPaletteColors,
    alpha: f32,
) {
    let w = img.width() as f32;
    let h = img.height() as f32;
    let cam = &spec.camera;
    let ink = parse_hex(&colors.guide);
    for volume in &spec.volumes {
        let part = volume_as_part(volume);
        let poly = part_poly(&part, w, h, cam);
        if poly.len() < 3 {
            continue;
        }
        for pair in poly.windows(2) {
            draw_line(
                img,
                pair[0].0 as f32,
                pair[0].1 as f32,
                pair[1].0 as f32,
                pair[1].1 as f32,
                ink,
                alpha,
                1.6,
            );
        }
        if let (Some(first), Some(last)) = (poly.first(), poly.last()) {
            draw_line(
                img,
                last.0 as f32,
                last.1 as f32,
                first.0 as f32,
                first.1 as f32,
                ink,
                alpha,
                1.6,
            );
        }
    }
}

fn paint_contour_underlay(
    img: &mut RgbImage,
    spec: &IllustrationSpec,
    colors: &IllustrationPaletteColors,
    alpha: f32,
) {
    let w = img.width() as f32;
    let h = img.height() as f32;
    let cam = &spec.camera;
    let ink = mix(parse_hex(&colors.ink), parse_hex(&colors.paper), 0.55);
    for part in &spec.contours {
        let poly = part_poly(part, w, h, cam);
        if poly.len() < 3 {
            continue;
        }
        for pair in poly.windows(2) {
            draw_line(
                img,
                pair[0].0 as f32,
                pair[0].1 as f32,
                pair[1].0 as f32,
                pair[1].1 as f32,
                ink,
                alpha,
                1.8,
            );
        }
        if let (Some(first), Some(last)) = (poly.first(), poly.last()) {
            draw_line(
                img,
                last.0 as f32,
                last.1 as f32,
                first.0 as f32,
                first.1 as f32,
                ink,
                alpha,
                1.8,
            );
        }
    }
}

fn paint_construction_phase(
    img: &mut RgbImage,
    spec: &IllustrationSpec,
    colors: &IllustrationPaletteColors,
    progress: Option<f32>,
) {
    let guide = parse_hex(&colors.guide);
    let blush = parse_hex(&colors.blush);
    match spec.construction_phase {
        IllustrationConstructionPhase::Skeleton => {
            paint_action_line_and_ovals(img, spec, colors, progress);
            let skeleton_progress = match progress {
                None => None,
                Some(p) if p < 0.32 => Some(0.0),
                Some(p) => Some(((p - 0.32) / 0.68).clamp(0.0, 1.0)),
            };
            if skeleton_progress != Some(0.0) {
                draw_skeleton_pose_alpha(img, spec, guide, blush, 0.95, 0.95, skeleton_progress);
            }
        }
        IllustrationConstructionPhase::Volumes
        | IllustrationConstructionPhase::Contours
        | IllustrationConstructionPhase::Details => {
            // Previous construction stays visible under the active pass.
            if !spec.skeleton.is_empty() {
                let (bone_a, joint_a) = match spec.construction_phase {
                    IllustrationConstructionPhase::Volumes => (0.38, 0.38),
                    IllustrationConstructionPhase::Contours => (0.22, 0.22),
                    _ => (0.14, 0.14),
                };
                draw_skeleton_pose_alpha(img, spec, guide, blush, bone_a, joint_a, None);
            }
            if matches!(
                spec.construction_phase,
                IllustrationConstructionPhase::Contours | IllustrationConstructionPhase::Details
            ) && !spec.volumes.is_empty()
            {
                let alpha = if spec.construction_phase == IllustrationConstructionPhase::Contours {
                    0.34
                } else {
                    0.2
                };
                paint_volume_underlay(img, spec, colors, alpha);
            }
            if spec.construction_phase == IllustrationConstructionPhase::Details
                && !spec.contours.is_empty()
            {
                paint_contour_underlay(img, spec, colors, 0.28);
            }

            let mut staged = spec.clone();
            staged.construction_phase = IllustrationConstructionPhase::Final;
            staged.parts = match spec.construction_phase {
                IllustrationConstructionPhase::Volumes => {
                    let mut parts = spec
                        .parts
                        .iter()
                        .filter(|part| part_is_background(part))
                        .cloned()
                        .collect::<Vec<_>>();
                    parts.extend(spec.volumes.iter().map(volume_as_part));
                    parts
                }
                IllustrationConstructionPhase::Contours => {
                    let mut parts = background_parts(spec);
                    parts.extend(spec.contours.clone());
                    parts
                }
                IllustrationConstructionPhase::Details => {
                    let mut parts = background_parts(spec);
                    parts.extend(spec.details.clone());
                    parts
                }
                _ => Vec::new(),
            };
            paint_spec(img, &staged, colors, false, progress);
            if spec.construction_phase == IllustrationConstructionPhase::Volumes
                && progress.map(|p| p >= 0.88).unwrap_or(true)
            {
                draw_volume_guides(img, spec, colors);
            }
        }
        IllustrationConstructionPhase::Final => {}
    }
}

fn draw_volume_guides(img: &mut RgbImage, spec: &IllustrationSpec, colors: &IllustrationPaletteColors) {
    let w = img.width() as f32;
    let h = img.height() as f32;
    let color = parse_hex(&colors.guide);
    for volume in &spec.volumes {
        let cx = volume.x + volume.w * 0.5;
        let cy = volume.y + volume.h * 0.5;
        match volume.kind.as_str() {
            "head_sphere" | "rib_cage" | "pelvis" => {
                let (s, c) = volume.rotation.sin_cos();
                let project = |x: f32, y: f32| ((cx + x*c - y*s)*w, (cy + x*s + y*c)*h);
                let eye_y = if volume.kind == "head_sphere" { -volume.h * 0.08 } else { 0.0 };
                for (a, b) in [
                    (project(0.0, -volume.h*0.5), project(0.0, volume.h*0.5)),
                    (project(-volume.w*0.5, eye_y), project(volume.w*0.5, eye_y)),
                ] {
                    draw_line(img, a.0, a.1, b.0, b.1, color, 0.28, 1.0);
                }
            }
            "cylinder" => {
                let length = volume.w.max(volume.h) * 0.42;
                let ca = volume.rotation.cos();
                let sa = volume.rotation.sin();
                draw_line(
                    img,
                    (cx - length * ca) * w,
                    (cy - length * sa) * h,
                    (cx + length * ca) * w,
                    (cy + length * sa) * h,
                    color,
                    0.22,
                    0.9,
                );
            }
            _ => {}
        }
    }
}

fn draw_construction_oval(
    img: &mut RgbImage,
    oval: &aos_proto::IllustrationConstructionOval,
    w: f32,
    h: f32,
    color: Rgb<u8>,
) {
    let mut points = Vec::with_capacity(25);
    let ca = oval.rotation.cos();
    let sa = oval.rotation.sin();
    for i in 0..=24 {
        let t = i as f32 / 24.0 * TAU;
        let x = t.cos() * oval.w * 0.5;
        let y = t.sin() * oval.h * 0.5;
        points.push((
            oval.cx + x * ca - y * sa,
            oval.cy + x * sa + y * ca,
        ));
    }
    for pair in points.windows(2) {
        draw_line(
            img,
            pair[0].0 * w,
            pair[0].1 * h,
            pair[1].0 * w,
            pair[1].1 * h,
            color,
            0.72,
            1.4,
        );
    }
    draw_line(
        img,
        (oval.cx - oval.w * 0.5) * w,
        oval.cy * h,
        (oval.cx + oval.w * 0.5) * w,
        oval.cy * h,
        color,
        0.42,
        1.0,
    );
}

fn background_parts(spec: &IllustrationSpec) -> Vec<IllustrationPart> {
    spec.parts
        .iter()
        .filter(|part| part_is_background(part))
        .cloned()
        .collect()
}

fn volume_as_part(volume: &aos_proto::IllustrationVolume) -> IllustrationPart {
    let cx = volume.x + volume.w * 0.5;
    let cy = volume.y + volume.h * 0.5;
    let path = |points: Vec<(f32, f32)>| IllustrationPartGeometry::Path {
        points: points
            .into_iter()
            .map(|(x, y)| {
                // Cylinders already construct their contour in the rotated frame.
                if volume.kind == "cylinder" {
                    aos_proto::CanvasPoint { x, y }
                } else {
                    let (s, c) = volume.rotation.sin_cos();
                    aos_proto::CanvasPoint {
                        x: cx + (x - cx) * c - (y - cy) * s,
                        y: cy + (x - cx) * s + (y - cy) * c,
                    }
                }
            })
            .collect(),
        closed: true,
    };
    let geometry = match volume.kind.as_str() {
        "rib_cage" => path(vec![
            (cx - volume.w * 0.28, volume.y),
            (cx + volume.w * 0.28, volume.y),
            (volume.x + volume.w * 0.88, volume.y + volume.h * 0.18),
            (volume.x + volume.w, volume.y + volume.h * 0.50),
            (volume.x + volume.w * 0.84, volume.y + volume.h * 0.88),
            (cx + volume.w * 0.25, volume.y + volume.h),
            (cx - volume.w * 0.25, volume.y + volume.h),
            (volume.x + volume.w * 0.16, volume.y + volume.h * 0.88),
            (volume.x, volume.y + volume.h * 0.50),
            (volume.x + volume.w * 0.12, volume.y + volume.h * 0.18),
        ]),
        "pelvis" => path(vec![
            (cx - volume.w * 0.32, volume.y),
            (cx + volume.w * 0.32, volume.y),
            (volume.x + volume.w * 0.90, volume.y + volume.h * 0.32),
            (volume.x + volume.w * 0.74, volume.y + volume.h * 0.78),
            (volume.x + volume.w * 0.52, volume.y + volume.h),
            (volume.x + volume.w * 0.48, volume.y + volume.h),
            (volume.x + volume.w * 0.26, volume.y + volume.h * 0.78),
            (volume.x + volume.w * 0.10, volume.y + volume.h * 0.32),
        ]),
        "head_sphere" => path(vec![
            (cx, volume.y),
            (volume.x + volume.w * 0.76, volume.y + volume.h * 0.10),
            (volume.x + volume.w, volume.y + volume.h * 0.40),
            (volume.x + volume.w * 0.88, volume.y + volume.h * 0.74),
            (volume.x + volume.w * 0.62, volume.y + volume.h * 0.92),
            (cx, volume.y + volume.h),
            (volume.x + volume.w * 0.34, volume.y + volume.h * 0.90),
            (volume.x + volume.w * 0.10, volume.y + volume.h * 0.70),
            (volume.x, volume.y + volume.h * 0.38),
            (volume.x + volume.w * 0.24, volume.y + volume.h * 0.10),
        ]),
        "cylinder" => {
            let length = volume.w.max(volume.h);
            let thickness = volume.w.min(volume.h).max(0.025);
            let ca = volume.rotation.cos();
            let sa = volume.rotation.sin();
            let corners = [
                (-length * 0.5 + thickness * 0.5, -thickness * 0.5),
                (length * 0.5 - thickness * 0.5, -thickness * 0.5),
                (length * 0.5 - thickness * 0.2, -thickness * 0.42),
                (length * 0.5, 0.0),
                (length * 0.5 - thickness * 0.2, thickness * 0.42),
                (length * 0.5 - thickness * 0.5, thickness * 0.5),
                (-length * 0.5 + thickness * 0.5, thickness * 0.5),
                (-length * 0.5 + thickness * 0.2, thickness * 0.42),
                (-length * 0.5, 0.0),
                (-length * 0.5 + thickness * 0.2, -thickness * 0.42),
            ];
            path(corners
                .into_iter()
                .map(|(x, y)| (cx + x * ca - y * sa, cy + x * sa + y * ca))
                .collect())
        }
        _ => path(vec![
            (cx - volume.w * 0.35, volume.y),
            (cx + volume.w * 0.28, volume.y + volume.h * 0.08),
            (volume.x + volume.w, volume.y + volume.h * 0.45),
            (volume.x + volume.w * 0.72, volume.y + volume.h),
            (volume.x + volume.w * 0.18, volume.y + volume.h * 0.90),
            (volume.x, volume.y + volume.h * 0.40),
        ]),
    };
    IllustrationPart {
        id: format!("volume_{}", volume.id),
        role: volume.kind.clone(),
        fill_index: 1,
        fill: false,
        outline: true,
        seed: 901,
        geometry,
    }
}

fn part_is_focal(spec: &IllustrationSpec, part: &IllustrationPart) -> bool {
    let focal = spec.construction.focal_point.to_ascii_lowercase();
    let id = part.id.to_ascii_lowercase();
    let role = part.role.to_ascii_lowercase();
    if focal.is_empty() {
        return false;
    }
    if focal.contains("pipe") || focal.contains("visage") {
        return id.contains("pipe") || id.contains("head") || id.contains("eye")
            || role == "head" || role == "eye";
    }
    if focal.contains("contact") || focal.contains("support") {
        return id == "body" || id.contains("sofa") || id.contains("cushion")
            || role == "body";
    }
    id.contains("head") || id.contains("eye") || role == "head" || role == "body"
}

/// One conservative Chaikin pass for authored closed contours. This makes
/// model-provided paths read as designed curves while preserving short paths
/// used for gestures, paws and thin props.
fn smooth_contour(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let n = points.len();
    if n < 8 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        out.push((a.0 * 0.75 + b.0 * 0.25, a.1 * 0.75 + b.1 * 0.25));
        out.push((a.0 * 0.25 + b.0 * 0.75, a.1 * 0.25 + b.1 * 0.75));
    }
    out
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
            let gap = 12.0 / density;
            hatch(img, poly, angle, gap, 8.0, shade_c, 0.11 * density, seed);
            grain(img, Some(poly), 36, shade_c, 0.08, seed.wrapping_add(1));
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
        IllustrationFinish::Pencil => (span * 0.028, 1.25, 1u32),
        IllustrationFinish::Ink | IllustrationFinish::Screen => (span * 0.12, 2.15, 2),
        IllustrationFinish::Riso => (span * 0.11, 2.5, 3),
        IllustrationFinish::Flat => (span * 0.04, 1.15, 1),
    };
    let amp = (amp * amp_scale).clamp(0.8, 14.0);
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
    export_frame_fx(
        base,
        pose,
        camera,
        mode,
        width,
        height,
        &IllustrationFrameFx::default(),
    )
}

/// Extra film devices for one drawn frame. All default to off.
#[derive(Debug, Clone, Default)]
pub struct IllustrationFrameFx {
    /// Stroke reveal, 0..1. `None` draws the finished frame.
    pub progress: Option<f32>,
    /// Iris open, 0..1. `None` skips the mask.
    pub iris: Option<f32>,
    pub sign_off: Option<String>,
    pub sign_progress: f32,
    /// Absolute walk amount. Speed lines appear above 0.55.
    pub speed: f32,
    /// Use a complete replacement drawing for this frame. This keeps its
    /// authored marks and overlaps stable across an exposure.
    pub preserve_drawing: bool,
}

pub fn export_frame_fx(
    base: &IllustrationDoc,
    pose: &aos_proto::IllustrationPose,
    camera: &aos_proto::IllustrationCamera,
    mode: IllustrationRenderMode,
    width: u32,
    height: u32,
    fx: &IllustrationFrameFx,
) -> Result<Vec<u8>, String> {
    let mut doc = base.clone();
    if let Some(spec) = doc.spec.as_mut() {
        spec.pose = pose.clone();
        spec.camera = camera.clone();
        spec.mode = mode;
        if !fx.preserve_drawing {
            apply_pose_motion(spec, pose);
        }
        if let Some((bx, by)) = body_center(spec) {
            spec.camera.x = camera.x * 0.75 + bx * 0.25;
            spec.camera.y = camera.y * 0.75 + by * 0.25;
        }
        spec.camera.rot = camera.rot;
        spec.camera.zoom = camera.zoom;
    }
    let bytes = if let Some(p) = fx.progress {
        export_png_progress(&doc, width, height, p)?
    } else {
        export_png(&doc, width, height)?
    };
    let mut img = image::load_from_memory(&bytes)
        .map_err(|e| e.to_string())?
        .to_rgb8();
    let colors = aos_proto::IllustrationPaletteColors::for_preset(doc.brief.palette);
    let paper = parse_hex(&colors.paper);
    if let Some(iris) = fx.iris {
        apply_iris(&mut img, iris, paper);
    }
    if fx.speed > 0.55 {
        paint_speed_lines(&mut img, parse_hex(&colors.ink), fx.speed);
    }
    if let Some(word) = fx.sign_off.as_deref() {
        paint_sign_off(&mut img, word, fx.sign_progress, parse_hex(&colors.ink));
    }
    encode_png(&img)
}

fn body_center(spec: &IllustrationSpec) -> Option<(f32, f32)> {
    let part = spec.parts.iter().find(|p| {
        let id = p.id.to_ascii_lowercase();
        !part_is_background(p)
            && !part_is_furniture(p)
            && (id.contains("body") || id.contains("corps") || p.role == "body")
    })?;
    Some(part_center(part))
}

fn part_center(part: &IllustrationPart) -> (f32, f32) {
    match &part.geometry {
        IllustrationPartGeometry::Ellipse { x, y, w, h, .. }
        | IllustrationPartGeometry::Rect { x, y, w, h, .. } => (x + w * 0.5, y + h * 0.5),
        IllustrationPartGeometry::Path { points, .. } => {
            if points.is_empty() {
                return (0.5, 0.5);
            }
            let n = points.len() as f32;
            (
                points.iter().map(|p| p.x).sum::<f32>() / n,
                points.iter().map(|p| p.y).sum::<f32>() / n,
            )
        }
    }
}

/// Move named parts. Furniture and the background stay put. The camera is not the actor.
fn apply_pose_motion(spec: &mut IllustrationSpec, pose: &aos_proto::IllustrationPose) {
    for part in &mut spec.parts {
        if part_is_background(part) || part_is_furniture(part) {
            continue;
        }
        let id = part.id.to_ascii_lowercase();
        let role = part.role.to_ascii_lowercase();
        let (cx, cy) = part_center(part);
        if id.contains("wheel") {
            transform_geometry(&mut part.geometry, 0.0, 0.0, pose.walk * 0.9, cx, cy);
            continue;
        }
        if id.contains("ear")
            || id.contains("tail")
            || id.contains("trunk")
            || id.contains("wing")
            || id.contains("beak")
            || id.contains("queue")
        {
            transform_geometry(
                &mut part.geometry,
                pose.flap * 0.035,
                pose.flap * 0.02,
                pose.flap * 0.45,
                cx,
                cy,
            );
            continue;
        }
        if role == "leg" || id.contains("paw") || id.contains("patte") || id.contains("leg") {
            let (_, top) = match &part.geometry {
                IllustrationPartGeometry::Ellipse { y, .. }
                | IllustrationPartGeometry::Rect { y, .. } => (cx, *y),
                _ => (cx, cy),
            };
            transform_geometry(
                &mut part.geometry,
                0.0,
                0.0,
                pose.walk * 0.65 + pose.tuck * 0.85,
                cx,
                top,
            );
            continue;
        }
        let dy = -pose.tuck * 0.18 + pose.twitch * 0.025;
        let dx = pose.wing * 0.02;
        transform_geometry(&mut part.geometry, dx, dy, pose.tilt * 0.22, cx, cy);
    }
}

fn transform_geometry(
    geo: &mut IllustrationPartGeometry,
    dx: f32,
    dy: f32,
    ang: f32,
    pivot_x: f32,
    pivot_y: f32,
) {
    let map = |x: f32, y: f32| -> (f32, f32) {
        let (c, s) = (ang.cos(), ang.sin());
        let vx = x - pivot_x;
        let vy = y - pivot_y;
        (
            pivot_x + vx * c - vy * s + dx,
            pivot_y + vx * s + vy * c + dy,
        )
    };
    match geo {
        IllustrationPartGeometry::Ellipse {
            x,
            y,
            w,
            h,
            rotation,
        }
        | IllustrationPartGeometry::Rect {
            x,
            y,
            w,
            h,
            rotation,
        } => {
            let (cx, cy) = map(*x + *w * 0.5, *y + *h * 0.5);
            *x = cx - *w * 0.5;
            *y = cy - *h * 0.5;
            *rotation += ang;
        }
        IllustrationPartGeometry::Path { points, .. } => {
            for p in points {
                let (nx, ny) = map(p.x, p.y);
                p.x = nx;
                p.y = ny;
            }
        }
    }
}

fn apply_iris(img: &mut RgbImage, t: f32, paper: Rgb<u8>) {
    let t = t.clamp(0.0, 1.0);
    let w = img.width() as f32;
    let h = img.height() as f32;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let max_r = (w * w + h * h).sqrt() * 0.55;
    let r = max_r * t;
    for y in 0..img.height() {
        for x in 0..img.width() {
            let d = (x as f32 - cx).hypot(y as f32 - cy);
            if d > r {
                img.put_pixel(x, y, paper);
            }
        }
    }
}

fn paint_speed_lines(img: &mut RgbImage, ink: Rgb<u8>, speed: f32) {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let n = 5i32;
    for i in 0..n {
        let y = h / 3 + i * (h / 18);
        let x1 = w / 12;
        let x2 = x1 + (w as f32 * 0.18 * speed.min(1.4)) as i32;
        draw_line(img, x1 as f32, y as f32, x2 as f32, y as f32, ink, 0.55, 1.4);
    }
}

fn paint_sign_off(img: &mut RgbImage, word: &str, progress: f32, ink: Rgb<u8>) {
    let progress = progress.clamp(0.0, 1.0);
    if progress <= 0.01 || word.is_empty() {
        return;
    }
    let glyphs: Vec<char> = word.chars().collect();
    let shown = ((glyphs.len() as f32) * progress).ceil() as usize;
    let scale = (img.width() as f32 * 0.045).max(8.0);
    let x0 = img.width() as f32 * 0.62;
    let y0 = img.height() as f32 * 0.78;
    for (i, ch) in glyphs.iter().take(shown).enumerate() {
        for (x1, y1, x2, y2) in glyph(*ch) {
            draw_line(
                img,
                x0 + (i as f32) * scale * 1.3 + x1 * scale,
                y0 + y1 * scale,
                x0 + (i as f32) * scale * 1.3 + x2 * scale,
                y0 + y2 * scale,
                ink,
                0.9,
                1.6,
            );
        }
    }
}

fn glyph(ch: char) -> &'static [(f32, f32, f32, f32)] {
    match ch {
        'a' => &[(0.15, 0.45, 0.8, 0.45), (0.8, 0.45, 0.8, 1.0), (0.15, 0.7, 0.8, 0.7), (0.15, 0.45, 0.15, 1.0)],
        'b' => &[(0.15, 0.0, 0.15, 1.0), (0.15, 0.45, 0.75, 0.55), (0.75, 0.55, 0.15, 1.0)],
        'c' => &[(0.8, 0.45, 0.2, 0.45), (0.2, 0.45, 0.2, 1.0), (0.2, 1.0, 0.8, 1.0)],
        'd' => &[(0.75, 0.0, 0.75, 1.0), (0.75, 0.45, 0.2, 0.55), (0.2, 0.55, 0.75, 1.0)],
        'e' => &[(0.8, 0.45, 0.15, 0.45), (0.15, 0.45, 0.15, 1.0), (0.15, 0.72, 0.65, 0.72), (0.15, 1.0, 0.8, 1.0)],
        'f' => &[(0.7, 0.05, 0.25, 0.05), (0.25, 0.05, 0.25, 1.0), (0.1, 0.45, 0.7, 0.45)],
        'g' => &[(0.8, 0.45, 0.2, 0.45), (0.2, 0.45, 0.2, 1.0), (0.2, 1.0, 0.8, 1.0), (0.8, 1.0, 0.8, 0.7)],
        'h' => &[(0.15, 0.0, 0.15, 1.0), (0.15, 0.55, 0.8, 0.55), (0.8, 0.55, 0.8, 1.0)],
        'i' => &[(0.45, 0.15, 0.45, 0.28), (0.45, 0.45, 0.45, 1.0)],
        'j' => &[(0.55, 0.15, 0.55, 0.28), (0.55, 0.45, 0.55, 0.95), (0.55, 0.95, 0.2, 1.0)],
        'k' => &[(0.15, 0.0, 0.15, 1.0), (0.75, 0.45, 0.15, 0.72), (0.35, 0.68, 0.8, 1.0)],
        'l' => &[(0.35, 0.0, 0.35, 1.0), (0.35, 1.0, 0.7, 1.0)],
        'm' => &[(0.1, 1.0, 0.1, 0.45), (0.1, 0.45, 0.45, 0.7), (0.45, 0.7, 0.85, 0.45), (0.85, 0.45, 0.85, 1.0)],
        'n' => &[(0.15, 1.0, 0.15, 0.45), (0.15, 0.5, 0.8, 0.5), (0.8, 0.5, 0.8, 1.0)],
        'o' => &[(0.2, 0.45, 0.8, 0.45), (0.8, 0.45, 0.8, 1.0), (0.8, 1.0, 0.2, 1.0), (0.2, 1.0, 0.2, 0.45)],
        'p' => &[(0.15, 0.45, 0.15, 1.25), (0.15, 0.45, 0.75, 0.55), (0.75, 0.55, 0.15, 0.85)],
        'r' => &[(0.2, 1.0, 0.2, 0.45), (0.2, 0.5, 0.75, 0.45)],
        's' => &[(0.8, 0.5, 0.2, 0.45), (0.2, 0.45, 0.75, 0.72), (0.75, 0.72, 0.2, 1.0)],
        't' => &[(0.15, 0.2, 0.85, 0.2), (0.5, 0.2, 0.5, 1.0)],
        'u' => &[(0.15, 0.45, 0.15, 0.95), (0.15, 0.95, 0.8, 0.95), (0.8, 0.95, 0.8, 0.45)],
        'v' => &[(0.1, 0.45, 0.45, 1.0), (0.45, 1.0, 0.85, 0.45)],
        'w' => &[(0.05, 0.45, 0.25, 1.0), (0.25, 1.0, 0.5, 0.6), (0.5, 0.6, 0.75, 1.0), (0.75, 1.0, 0.95, 0.45)],
        'x' => &[(0.15, 0.45, 0.8, 1.0), (0.8, 0.45, 0.15, 1.0)],
        'y' => &[(0.15, 0.45, 0.5, 0.75), (0.85, 0.45, 0.3, 1.15)],
        'z' => &[(0.15, 0.45, 0.85, 0.45), (0.85, 0.45, 0.15, 1.0), (0.15, 1.0, 0.85, 1.0)],
        '0'..='9' => &[(0.2, 0.45, 0.8, 0.45), (0.8, 0.45, 0.8, 1.0), (0.8, 1.0, 0.2, 1.0), (0.2, 1.0, 0.2, 0.45)],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aos_proto::{
        IllustrationBrief, IllustrationLook, IllustrationPaletteId, IllustrationPart,
        IllustrationPartGeometry, IllustrationSkeletonJoint, IllustrationSpec,
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
    fn joint_bound_render_matches_explicit_world_contour() {
        let mut spec = IllustrationSpec::default();
        spec.skeleton = vec![
            aos_proto::IllustrationSkeletonJoint { id:"a".into(), x:0.4, y:0.5, ..Default::default() },
            aos_proto::IllustrationSkeletonJoint { id:"b".into(), x:0.4, y:0.3, ..Default::default() },
        ];
        spec.parts = serde_json::from_value(serde_json::json!([{
            "id":"sleeve","role":"clothing","fill":true,"outline":true,
            "geometry":{"kind":"path","closed":true,"points":[
                {"x":0.0,"y":-0.1},{"x":1.0,"y":-0.1},
                {"x":1.0,"y":0.1},{"x":0.0,"y":0.1}]}
        }])).unwrap();
        spec.joint_bindings.insert("sleeve".into(), ["a".into(),"b".into()]);
        let mut doc = IllustrationDoc { spec:Some(spec.clone()), ..Default::default() };
        let bound = export_png(&doc, 96, 96).unwrap();
        doc.spec = Some(spec.resolve_joint_bindings().unwrap());
        assert_eq!(bound, export_png(&doc, 96, 96).unwrap());
        spec.skeleton[1].x = 0.6;
        spec.skeleton[1].y = 0.5;
        doc.spec = Some(spec);
        assert_ne!(bound, export_png(&doc, 96, 96).unwrap());
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
    fn model_sheet_contains_six_reference_cells() {
        let mut spec = IllustrationSpec::default();
        spec.brief.subject = "un chat sur un canapé".into();
        aos_proto::enrich_illustration_puppet(&mut spec);
        let doc = IllustrationDoc {
            brief: spec.brief.clone(),
            spec: Some(spec),
            ..Default::default()
        };
        let png = export_model_sheet_png(&doc, 180).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!(decoded.width(), 540);
        assert_eq!(decoded.height(), 360);
    }

    #[test]
    fn model_sheet_views_remove_front_facing_features() {
        let mut spec = IllustrationSpec::default();
        spec.brief.subject = "un chat sur un canapé".into();
        aos_proto::enrich_illustration_puppet(&mut spec);
        let profile = model_view_spec(&spec, "profil");
        assert!(!profile.parts.iter().any(|p| p.id.ends_with("eye_r")));
        assert!(!profile.parts.iter().any(|p| p.id.ends_with("ear_r")));
        let back = model_view_spec(&spec, "dos");
        assert!(!back.parts.iter().any(|p| p.id.contains("eye")));
        assert!(!back.parts.iter().any(|p| p.id.contains("snout")));
    }

    #[test]
    fn empty_parts_still_draw_the_skeleton() {
        let mut bare = IllustrationSpec::default();
        bare.brief.look = IllustrationLook::Pencil;
        bare.brief.palette = IllustrationPaletteId::PencilMinimal;
        let mut posed = bare.clone();
        posed.skeleton = vec![
            IllustrationSkeletonJoint { id: "pelvis".into(), x: 0.46, y: 0.58, ..Default::default() },
            IllustrationSkeletonJoint {
                id: "head".into(),
                parent: Some("pelvis".into()),
                x: 0.46,
                y: 0.26,
                ..Default::default()
            },
        ];
        let render = |spec: IllustrationSpec| {
            let doc = IllustrationDoc { brief: spec.brief.clone(), spec: Some(spec), ..Default::default() };
            let png = rasterize(&doc, 160, 160, None).unwrap();
            image::load_from_memory(&png).unwrap().to_rgb8()
        };
        let blank = render(bare);
        let drawn = render(posed);
        let changed = blank.pixels().zip(drawn.pixels()).filter(|(a, b)| a != b).count();
        assert!(changed > 40, "undressed skeleton must mark the paper, changed={changed}");
    }

    #[test]
    fn construction_phases_are_renderable() {
        let mut spec = IllustrationSpec::default();
        spec.brief.subject = "un chat saute sur un canapé et se couche".into();
        aos_proto::enrich_illustration_puppet(&mut spec);
        for phase in [
            IllustrationConstructionPhase::Skeleton,
            IllustrationConstructionPhase::Volumes,
            IllustrationConstructionPhase::Contours,
            IllustrationConstructionPhase::Details,
        ] {
            spec.construction_phase = phase;
            let doc = IllustrationDoc {
                brief: spec.brief.clone(),
                spec: Some(spec.clone()),
                ..Default::default()
            };
            let png = export_png(&doc, 192, 192).unwrap();
            assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        }
    }

    #[test]
    fn skeleton_progress_reveals_more_over_time() {
        let mut spec = IllustrationSpec::default();
        spec.brief.look = IllustrationLook::Pencil;
        spec.brief.palette = IllustrationPaletteId::PencilMinimal;
        spec.construction_phase = IllustrationConstructionPhase::Skeleton;
        spec.skeleton = vec![
            IllustrationSkeletonJoint {
                id: "pelvis".into(),
                x: 0.5,
                y: 0.62,
                ..Default::default()
            },
            IllustrationSkeletonJoint {
                id: "chest".into(),
                parent: Some("pelvis".into()),
                x: 0.5,
                y: 0.42,
                ..Default::default()
            },
            IllustrationSkeletonJoint {
                id: "head".into(),
                parent: Some("chest".into()),
                x: 0.5,
                y: 0.22,
                ..Default::default()
            },
            IllustrationSkeletonJoint {
                id: "l_hand".into(),
                parent: Some("chest".into()),
                x: 0.28,
                y: 0.48,
                ..Default::default()
            },
            IllustrationSkeletonJoint {
                id: "r_hand".into(),
                parent: Some("chest".into()),
                x: 0.72,
                y: 0.48,
                ..Default::default()
            },
        ];
        let doc = IllustrationDoc {
            brief: spec.brief.clone(),
            spec: Some(spec),
            ..Default::default()
        };
        let count = |png: &[u8]| {
            let img = image::load_from_memory(png).unwrap().to_rgb8();
            let blank = {
                let empty = IllustrationDoc {
                    brief: doc.brief.clone(),
                    ..Default::default()
                };
                image::load_from_memory(&export_png(&empty, 160, 160).unwrap())
                    .unwrap()
                    .to_rgb8()
            };
            blank.pixels().zip(img.pixels()).filter(|(a, b)| a != b).count()
        };
        let early = count(&export_png_progress(&doc, 160, 160, 0.15).unwrap());
        let mid = count(&export_png_progress(&doc, 160, 160, 0.55).unwrap());
        let late = count(&export_png_progress(&doc, 160, 160, 0.95).unwrap());
        assert!(
            early < mid && mid <= late,
            "skeleton must draw in over progress: early={early} mid={mid} late={late}"
        );
    }

    #[test]
    fn volumes_pass_keeps_skeleton_underlay() {
        let mut skeleton_only = IllustrationSpec::default();
        skeleton_only.brief.look = IllustrationLook::Pencil;
        skeleton_only.brief.palette = IllustrationPaletteId::PencilMinimal;
        skeleton_only.construction_phase = IllustrationConstructionPhase::Skeleton;
        skeleton_only.skeleton = vec![
            IllustrationSkeletonJoint {
                id: "pelvis".into(),
                x: 0.5,
                y: 0.6,
                ..Default::default()
            },
            IllustrationSkeletonJoint {
                id: "head".into(),
                parent: Some("pelvis".into()),
                x: 0.5,
                y: 0.25,
                ..Default::default()
            },
        ];
        let mut volumes = skeleton_only.clone();
        volumes.construction_phase = IllustrationConstructionPhase::Volumes;
        volumes.volumes = vec![aos_proto::IllustrationVolume {
            id: "rib".into(),
            kind: "rib_cage".into(),
            x: 0.35,
            y: 0.32,
            w: 0.3,
            h: 0.28,
            rotation: 0.0,
        }];
        let render = |spec: IllustrationSpec| {
            let doc = IllustrationDoc {
                brief: spec.brief.clone(),
                spec: Some(spec),
                ..Default::default()
            };
            image::load_from_memory(&export_png(&doc, 160, 160).unwrap())
                .unwrap()
                .to_rgb8()
        };
        let skel = render(skeleton_only);
        let vol = render(volumes);
        let changed = skel.pixels().zip(vol.pixels()).filter(|(a, b)| a != b).count();
        assert!(
            changed > 80,
            "volumes pass must add mass on top of skeleton, changed={changed}"
        );
    }

    #[test]
    fn semantic_volumes_use_constructive_paths() {
        for kind in ["rib_cage", "pelvis", "head_sphere", "cylinder", "organic_mass"] {
            let volume = aos_proto::IllustrationVolume {
                id: kind.into(),
                kind: kind.into(),
                x: 0.2,
                y: 0.2,
                w: 0.3,
                h: 0.4,
                rotation: 0.4,
            };
            assert!(matches!(
                volume_as_part(&volume).geometry,
                IllustrationPartGeometry::Path { points, closed: true } if points.len() >= 4
            ));
        }
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

    #[test]
    fn pose_moves_the_frame() {
        let mut doc = IllustrationDoc {
            brief: IllustrationBrief {
                subject: "blob".into(),
                look: IllustrationLook::Pencil,
                palette: IllustrationPaletteId::PencilMinimal,
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
                    x: 0.22,
                    y: 0.28,
                    w: 0.4,
                    h: 0.32,
                    rotation: 0.0,
                },
            }],
            ..Default::default()
        });
        let cam = aos_proto::IllustrationCamera::default();
        let still = export_frame_png(
            &doc,
            &aos_proto::IllustrationPose::default(),
            &cam,
            IllustrationRenderMode::Normal,
            96,
            96,
        )
        .unwrap();
        let moved = export_frame_png(
            &doc,
            &aos_proto::IllustrationPose {
                twitch: 1.0,
                tilt: 0.8,
                flap: 0.6,
                ..Default::default()
            },
            &cam,
            IllustrationRenderMode::Normal,
            96,
            96,
        )
        .unwrap();
        assert_ne!(still, moved);
    }

    #[test]
    fn person_pipe_preview_png() {
        let mut spec = IllustrationSpec {
            brief: IllustrationBrief {
                subject: "un homme serein fumant une pipe dans son jardin luxuriant".into(),
                look: IllustrationLook::Pencil,
                palette: IllustrationPaletteId::PencilMinimal,
                ..Default::default()
            },
            ..Default::default()
        };
        aos_proto::enrich_illustration_puppet(&mut spec);
        assert!(spec.parts.iter().any(|p| p.id == "pipe_stem"));
        assert!(spec.parts.iter().any(|p| p.id == "head"));
        let doc = IllustrationDoc {
            brief: spec.brief.clone(),
            spec: Some(spec),
            ..Default::default()
        };
        let png = export_png(&doc, 768, 768).unwrap();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/person-pipe-garden.png");
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(&path, &png).unwrap();
        assert!(png.len() > 5000);
    }
}
