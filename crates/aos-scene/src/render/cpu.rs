//! CPU SceneGraph wireframe / flat beauty / NPR approximation backend.
//!
//! When `RenderRequest.style` is set (Sketch / Pencil / Ink), draws a paper
//! background with style-aware line art and optional hatching. Without a style,
//! keeps the legacy dark wireframe / flat beauty look.

use super::backend::{
    RenderBackend, RenderBackendId, RenderError, RenderOutput, RenderPassKind, RenderRequest,
};
use crate::camera::{active_camera_eye_target, fovy_from_hfov, hfov_rad};
use crate::light::{collect_lights, linear_to_srgb_u8, shade_diffuse, ResolvedLight};
use crate::math::{Mat4, Vec3};
use crate::mesh_asset::{
    default_mesh_search_roots, load_gltf_mesh, resolve_mesh_uri, CpuTriangleMesh,
};
use crate::png::encode_rgba8_png;
use crate::scene::{NodeKind, SceneGraph};
use crate::style::{ResolvedStyle, StyleFamily};
use std::collections::BTreeMap;

// The CPU renderer also builds in the WASM module, where the viewport feature is disabled.
/// Half-extent of unit `mesh_box` geometry (same as `viewport::mesh::UNIT_CUBE_HALF`).
const UNIT_CUBE_HALF: f32 = 0.5;

/// Soft caps to keep offline CPU renders cheap.
pub const CPU_MAX_EDGE: u32 = 512;

pub struct CpuWireframeBackend;

impl RenderBackend for CpuWireframeBackend {
    fn id(&self) -> RenderBackendId {
        RenderBackendId::Cpu
    }

    fn render(&self, req: &RenderRequest) -> Result<RenderOutput, RenderError> {
        let w = req.width.clamp(16, CPU_MAX_EDGE);
        let h = req.height.clamp(16, CPU_MAX_EDGE);
        req.scene
            .validate()
            .map_err(|e| RenderError::Scene(e.to_string()))?;

        let (eye, target) = camera_eye_target(&req.scene);
        let aspect = w as f32 / (h as f32).max(1.0);
        let view = look_at_rh(eye, target, Vec3::UNIT_Y);
        let (fovy, near, far) = camera_projection(&req.scene, aspect);
        let proj = perspective_rh(fovy, aspect, near, far);
        let view_proj = proj * view;

        let style = req.style.as_ref();
        let bg = background_rgba(req.pass, style);
        let scale = if style.is_some_and(|s| s.antialias) {
            2
        } else {
            1
        };
        let draw_w = w * scale;
        let draw_h = h * scale;
        let mut rgba = vec![0u8; (draw_w * draw_h * 4) as usize];
        for px in rgba.as_chunks_mut::<4>().0 {
            *px = bg;
        }
        if let Some(st) = style {
            apply_paper_grain(&mut rgba, draw_w, draw_h, st);
        }

        let mut boxes: Vec<(String, f32)> = Vec::new();
        for id in req.scene.node_ids_depth_first() {
            let Some(node) = req.scene.nodes.get(&id) else {
                continue;
            };
            if !node.visible || !matches!(node.kind, NodeKind::MeshBox | NodeKind::MeshAsset) {
                continue;
            }
            let Ok(world) = req.scene.world_matrix(&id) else {
                continue;
            };
            let center = world.transform_point(Vec3::ZERO);
            let depth = (center - eye).length();
            boxes.push((id, depth));
        }
        boxes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut raster = Raster {
            rgba: &mut rgba,
            w: draw_w,
            h: draw_h,
            view_proj: &view_proj,
            seed: 0xA05C_E11Eu64,
            pixel_scale: scale as i32,
        };
        let lights = collect_lights(&req.scene);
        let search_roots = default_mesh_search_roots();
        let search_refs: Vec<_> = search_roots.iter().map(|p| p.as_path()).collect();
        for (id, _) in boxes {
            let Some(node) = req.scene.nodes.get(&id) else {
                continue;
            };
            let Ok(world) = req.scene.world_matrix(&id) else {
                continue;
            };
            if matches!(node.kind, NodeKind::MeshAsset) {
                let mesh = node
                    .mesh_uri
                    .as_deref()
                    .and_then(|uri| resolve_mesh_uri(uri, &search_refs))
                    .and_then(|path| load_gltf_mesh(&path).ok());
                if let Some(mesh) = mesh {
                    draw_mesh(
                        &mut raster,
                        &mesh,
                        &world,
                        eye,
                        req.pass,
                        style,
                        node.material.as_ref(),
                    );
                    continue;
                }
            }
            let corners = oriented_box_corners(&world);
            draw_box(
                &mut raster,
                &corners,
                req.pass,
                style,
                &lights,
                node.material.as_ref(),
            );
        }

        draw_static_effects(&mut raster, &req.scene.effects);
        if let Some(preset) = &req.preset {
            for pixel in raster.rgba.as_chunks_mut::<4>().0 {
                pixel[0] =
                    (f32::from(pixel[0]) * (1.0 + preset.color_warmth * 0.12)).min(255.0) as u8;
                pixel[2] = (f32::from(pixel[2]) * (1.0 - preset.color_warmth * 0.1)) as u8;
            }
        }

        let rgba = if scale == 2 {
            downsample_2x(&rgba, w, h)
        } else {
            rgba
        };
        let png = encode_rgba8_png(w, h, &rgba).map_err(RenderError::Encode)?;
        Ok(RenderOutput {
            png,
            width: w,
            height: h,
            backend_id: RenderBackendId::Cpu,
            pass: req.pass,
        })
    }
}

fn draw_static_effects(raster: &mut Raster<'_>, effects: &[crate::fx::SceneEffect]) {
    use crate::fx::EffectKind;
    for effect in effects {
        let center = Vec3::new(effect.position[0], effect.position[1], effect.position[2]);
        let Some((cx, cy)) = raster.project(center) else {
            continue;
        };
        let radius = (effect.radius * effect.intensity * raster.pixel_scale as f32 * 14.0)
            .round()
            .clamp(1.0, 80.0) as i32;
        match effect.kind {
            EffectKind::Fog | EffectKind::Smoke => {
                let color = if effect.kind == EffectKind::Fog {
                    [220, 226, 232, 12]
                } else {
                    [75, 78, 86, 20]
                };
                for y in -radius..=radius {
                    for x in -radius..=radius {
                        if x * x + y * y <= radius * radius {
                            raster.put_px(cx + x, cy + y, color);
                        }
                    }
                }
            }
            EffectKind::Particles | EffectKind::Fire => {
                let color = if effect.kind == EffectKind::Fire {
                    [255, 103, 16, 235]
                } else {
                    [255, 239, 189, 210]
                };
                let count = (effect.intensity * 24.0).round().clamp(4.0, 24.0) as i32;
                for i in 0..count {
                    let hash = hash_u32(i as u32 * 17 + 73);
                    let span = (radius * 2 + 1) as u32;
                    let ox = (hash % span) as i32 - radius;
                    let oy = ((hash >> 16) % span) as i32 - radius;
                    raster.put_px(cx + ox, cy + oy, color);
                }
            }
        }
    }
}

fn tint_rgba(color: &mut [u8; 4], material: Option<&crate::scene::MaterialOverride>) {
    if let Some(tint) = material.and_then(|m| m.tint) {
        for index in 0..3 {
            color[index] = (f32::from(color[index]) * tint[index])
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

fn draw_mesh(
    raster: &mut Raster<'_>,
    mesh: &CpuTriangleMesh,
    world: &Mat4,
    eye: Vec3,
    pass: RenderPassKind,
    style: Option<&ResolvedStyle>,
    material: Option<&crate::scene::MaterialOverride>,
) {
    let mut fill = style
        .map(|s| {
            let rgb = s.fill_rgb();
            [rgb[0], rgb[1], rgb[2], 255]
        })
        .unwrap_or([
            (mesh.base_color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (mesh.base_color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (mesh.base_color[2] * 255.0).clamp(0.0, 255.0) as u8,
            255,
        ]);
    tint_rgba(&mut fill, material);
    let stroke = style
        .map(|s| {
            let rgb = s.stroke_rgb();
            [rgb[0], rgb[1], rgb[2], 255]
        })
        .unwrap_or([38, 48, 56, 255]);
    // CPU beauty is a simplified geometry preview. Keep its raster work bounded
    // while preserving the actual mesh silhouette instead of a box proxy.
    let mut edges: BTreeMap<(u32, u32), Vec<(Vec3, bool)>> = BTreeMap::new();
    for tri in mesh.indices.as_chunks::<3>().0.iter().take(30_000) {
        let vertex = |index: u32| {
            let start = index as usize * 6;
            mesh.interleaved
                .get(start..start + 3)
                .map(|p| world.transform_point(Vec3::new(p[0], p[1], p[2])))
        };
        let (Some(a), Some(b), Some(c)) = (vertex(tri[0]), vertex(tri[1]), vertex(tri[2])) else {
            continue;
        };
        let show_fill = matches!(pass, RenderPassKind::Beauty)
            && !style.is_some_and(|s| matches!(s.family, StyleFamily::Sketch));
        if show_fill {
            raster.fill_tri(a, b, c, fill);
        }
        if style.is_some() {
            let normal = (b - a).cross(c - a).normalized().unwrap_or(Vec3::UNIT_Y);
            let center = (a + b + c) * (1.0 / 3.0);
            let facing = normal.dot(eye - center) >= 0.0;
            for (from, to) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                let key = (from.min(to), from.max(to));
                edges.entry(key).or_default().push((normal, facing));
            }
        } else if matches!(pass, RenderPassKind::Wireframe) {
            raster.stroke_line(a, b, stroke);
            raster.stroke_line(b, c, stroke);
            raster.stroke_line(c, a, stroke);
        }
    }
    if let Some(st) = style {
        for ((from, to), adjacent) in edges {
            let front_count = adjacent.iter().filter(|(_, front)| *front).count();
            let silhouette = is_silhouette(&adjacent, front_count);
            let crease = is_crease(&adjacent, front_count);
            if !silhouette && !crease {
                continue;
            }
            let sample = hash_u32(from.wrapping_mul(73856093) ^ to.wrapping_mul(19349663));
            if !silhouette && (sample as f32 / u32::MAX as f32) > st.line_density {
                continue;
            }
            let vertex = |index: u32| {
                let start = index as usize * 6;
                mesh.interleaved
                    .get(start..start + 3)
                    .map(|p| world.transform_point(Vec3::new(p[0], p[1], p[2])))
            };
            if let (Some(a), Some(b)) = (vertex(from), vertex(to)) {
                let thickness = (st.line_width * raster.pixel_scale as f32)
                    .round()
                    .clamp(1.0, 8.0) as i32;
                raster.stroke_line_thick(a, b, stroke, thickness);
            }
        }
    }
}

fn is_silhouette(adjacent: &[(Vec3, bool)], front_count: usize) -> bool {
    (adjacent.len() == 1 && front_count == 1) || (front_count > 0 && front_count < adjacent.len())
}

fn is_crease(adjacent: &[(Vec3, bool)], front_count: usize) -> bool {
    adjacent.len() == 2 && front_count > 0 && adjacent[0].0.dot(adjacent[1].0) < 0.65
}

fn downsample_2x(src: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let dst = ((y * w + x) * 4) as usize;
            for c in 0..4 {
                let sum: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                    .iter()
                    .map(|(dx, dy)| {
                        u32::from(src[((((y * 2 + dy) * w * 2) + (x * 2 + dx)) * 4) as usize + c])
                    })
                    .sum();
                out[dst + c] = (sum / 4) as u8;
            }
        }
    }
    out
}

fn background_rgba(pass: RenderPassKind, style: Option<&ResolvedStyle>) -> [u8; 4] {
    if let Some(st) = style {
        let t = st.paper_tint;
        return [t[0], t[1], t[2], 255];
    }
    match pass {
        RenderPassKind::Beauty => [28u8, 34, 42, 255],
        RenderPassKind::Wireframe => [18u8, 20, 24, 255],
    }
}

fn apply_paper_grain(rgba: &mut [u8], w: u32, h: u32, style: &ResolvedStyle) {
    let strength = match style.family {
        StyleFamily::Sketch => 18u8,
        StyleFamily::Pencil => 12u8,
        StyleFamily::Ink => 4u8,
    };
    for y in 0..h {
        for x in 0..w {
            let n = hash_u32(
                x.wrapping_mul(374761393)
                    .wrapping_add(y.wrapping_mul(668265263)),
            );
            let delta = ((n >> 8) as u8) % (strength.max(1));
            let i = ((y * w + x) * 4) as usize;
            for channel in rgba[i..i + 3].iter_mut() {
                let v = *channel as i16 - (delta as i16 / 2);
                *channel = v.clamp(0, 255) as u8;
            }
        }
    }
}

fn draw_box(
    raster: &mut Raster<'_>,
    corners: &[Vec3; 8],
    pass: RenderPassKind,
    style: Option<&ResolvedStyle>,
    lights: &[ResolvedLight],
    material: Option<&crate::scene::MaterialOverride>,
) {
    let Some(st) = style else {
        let line = match pass {
            RenderPassKind::Beauty => [200u8, 180, 140, 255],
            RenderPassKind::Wireframe => [160u8, 200, 220, 255],
        };
        if matches!(pass, RenderPassKind::Beauty) {
            let mut fill = lit_face_rgba(
                lights,
                corners[0],
                corners[1],
                corners[2],
                [70.0 / 255.0, 78.0 / 255.0, 90.0 / 255.0],
                [70, 78, 90, 255],
            );
            tint_rgba(&mut fill, material);
            raster.fill_quad(corners[0], corners[1], corners[2], corners[3], fill);
        }
        for (i, j) in BOX_EDGES {
            raster.stroke_line(corners[i], corners[j], line);
        }
        return;
    };

    let stroke = {
        let rgb = st.stroke_rgb();
        let a = (st.opacity * 255.0).round().clamp(0.0, 255.0) as u8;
        [rgb[0], rgb[1], rgb[2], a]
    };
    let mut fill_base = {
        let rgb = st.fill_rgb();
        [rgb[0], rgb[1], rgb[2], 255]
    };
    tint_rgba(&mut fill_base, material);

    let do_fill = match st.family {
        StyleFamily::Sketch => false,
        StyleFamily::Pencil | StyleFamily::Ink => matches!(pass, RenderPassKind::Beauty),
    };
    if do_fill {
        let albedo = [
            fill_base[0] as f32 / 255.0,
            fill_base[1] as f32 / 255.0,
            fill_base[2] as f32 / 255.0,
        ];
        let fill = lit_face_rgba(
            lights, corners[0], corners[1], corners[2], albedo, fill_base,
        );
        raster.fill_quad(corners[0], corners[1], corners[2], corners[3], fill);
        if st.shading != "none" {
            hatch_face(raster, corners[0], corners[1], corners[2], corners[3], st);
        }
    }

    let passes = match st.family {
        StyleFamily::Sketch => 2,
        StyleFamily::Pencil => 1,
        StyleFamily::Ink => 1,
    };
    let thickness = (st.line_width * raster.pixel_scale as f32)
        .round()
        .clamp(1.0, 8.0) as i32;
    for _ in 0..passes {
        for (index, (i, j)) in BOX_EDGES.into_iter().enumerate() {
            if index >= 4 && (hash_u32(index as u32) as f32 / u32::MAX as f32) > st.line_density {
                continue;
            }
            let a = jitter_point(corners[i], st.variation, raster.next_f32());
            let b = jitter_point(corners[j], st.variation, raster.next_f32());
            raster.stroke_line_thick(a, b, stroke, thickness);
        }
    }
}

fn lit_face_rgba(
    lights: &[ResolvedLight],
    a: Vec3,
    b: Vec3,
    c: Vec3,
    albedo_linear: [f32; 3],
    fallback: [u8; 4],
) -> [u8; 4] {
    let center = (a + b + c) * (1.0 / 3.0);
    let normal = (b - a).cross(c - a);
    match shade_diffuse(lights, center, normal, albedo_linear) {
        Some(lin) => [
            linear_to_srgb_u8(lin[0]),
            linear_to_srgb_u8(lin[1]),
            linear_to_srgb_u8(lin[2]),
            255,
        ],
        None => fallback,
    }
}

fn hatch_face(raster: &mut Raster<'_>, a: Vec3, b: Vec3, c: Vec3, d: Vec3, style: &ResolvedStyle) {
    let rgb = style.stroke_rgb();
    let alpha = ((0.35 + style.contrast * 0.4) * 255.0) as u8;
    let col = [rgb[0], rgb[1], rgb[2], alpha];
    let base_steps = match style.family {
        StyleFamily::Sketch => 0,
        StyleFamily::Pencil => 6,
        StyleFamily::Ink => 4,
    };
    let steps = (base_steps as f32 * style.hatching * 2.0).round() as usize;
    if steps == 0 {
        return;
    }
    for i in 0..steps {
        let t = (i as f32 + 0.5) / steps as f32;
        let p0 = lerp3(a, d, t);
        let p1 = lerp3(b, c, t);
        let j0 = jitter_point(p0, style.jitter * 0.5, raster.next_f32());
        let j1 = jitter_point(p1, style.jitter * 0.5, raster.next_f32());
        raster.stroke_line(j0, j1, col);
    }
    if style.shading == "cross_hatching" {
        for i in 0..steps {
            let t = (i as f32 + 0.5) / steps as f32;
            let p0 = lerp3(a, b, t);
            let p1 = lerp3(d, c, t);
            raster.stroke_line(p0, p1, col);
        }
    }
}

fn jitter_point(p: Vec3, amount: f32, r: f32) -> Vec3 {
    if amount <= 0.0 {
        return p;
    }
    let dx = (r - 0.5) * amount * 0.12;
    let dy = ((r * 7.13).fract() - 0.5) * amount * 0.12;
    let dz = ((r * 13.37).fract() - 0.5) * amount * 0.12;
    p + Vec3::new(dx, dy, dz)
}

fn lerp3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    a + (b - a) * t
}

fn camera_eye_target(scene: &SceneGraph) -> (Vec3, Vec3) {
    // Prefer a look distance that reaches into the scene (viewport default orbit
    // is ~8 m). The old fixed 4 m pulled the target short of ground/props and
    // mismatched the edit orbit strip.
    const DEFAULT_LOOK: f32 = 8.0;
    if let Some((eye, target)) = active_camera_eye_target(scene, DEFAULT_LOOK) {
        return (eye, target);
    }
    (Vec3::new(0.0, 1.5, 4.0), Vec3::new(0.0, 0.5, 0.0))
}

fn camera_projection(scene: &SceneGraph, aspect: f32) -> (f32, f32, f32) {
    if let Some(cam_id) = scene.active_camera.as_deref() {
        if let Some(node) = scene.nodes.get(cam_id) {
            if let Some(params) = node.camera.as_ref() {
                let hfov = hfov_rad(params);
                let fovy = fovy_from_hfov(hfov, aspect);
                return (
                    fovy,
                    params.near.max(0.01),
                    params.far.max(params.near + 1.0),
                );
            }
        }
    }
    (50.0_f32.to_radians(), 0.1, 200.0)
}

fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye)
        .normalized()
        .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
    let s = f.cross(up).normalized().unwrap_or(Vec3::UNIT_X);
    let u = s.cross(f);
    Mat4::from_cols(
        [s.x, u.x, -f.x, 0.0],
        [s.y, u.y, -f.y, 0.0],
        [s.z, u.z, -f.z, 0.0],
        [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
    )
}

fn perspective_rh(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = (far + near) / (near - far);
    m[11] = -1.0;
    m[14] = (2.0 * far * near) / (near - far);
    Mat4 { m }
}

/// World-space corners of the unit mesh_box after node TRS (viewport parity).
fn oriented_box_corners(world: &Mat4) -> [Vec3; 8] {
    let h = UNIT_CUBE_HALF;
    [
        world.transform_point(Vec3::new(-h, -h, -h)),
        world.transform_point(Vec3::new(h, -h, -h)),
        world.transform_point(Vec3::new(h, h, -h)),
        world.transform_point(Vec3::new(-h, h, -h)),
        world.transform_point(Vec3::new(-h, -h, h)),
        world.transform_point(Vec3::new(h, -h, h)),
        world.transform_point(Vec3::new(h, h, h)),
        world.transform_point(Vec3::new(-h, h, h)),
    ]
}

const BOX_EDGES: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352du32);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68bu32);
    x ^= x >> 16;
    x
}

struct Raster<'a> {
    rgba: &'a mut [u8],
    w: u32,
    h: u32,
    view_proj: &'a Mat4,
    seed: u64,
    pixel_scale: i32,
}

impl Raster<'_> {
    fn next_f32(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((self.seed >> 33) as u32) as f32 / u32::MAX as f32
    }

    fn project(&self, p: Vec3) -> Option<(i32, i32)> {
        let clip = self.view_proj.transform_point(p);
        if !clip.x.is_finite() || !clip.y.is_finite() {
            return None;
        }
        let x = (clip.x * 0.5 + 0.5) * self.w as f32;
        let y = (1.0 - (clip.y * 0.5 + 0.5)) * self.h as f32;
        Some((x.round() as i32, y.round() as i32))
    }

    fn put_px(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = ((y as u32 * self.w + x as u32) * 4) as usize;
        if color[3] >= 250 {
            self.rgba[i..i + 4].copy_from_slice(&color);
            return;
        }
        let a = color[3] as u16;
        for (channel, &src) in self.rgba[i..i + 3].iter_mut().zip(color[..3].iter()) {
            let dst = *channel as u16;
            *channel = ((src as u16 * a + dst * (255 - a)) / 255) as u8;
        }
        self.rgba[i + 3] = 255;
    }

    fn stroke_line(&mut self, a: Vec3, b: Vec3, color: [u8; 4]) {
        self.stroke_line_thick(a, b, color, self.pixel_scale);
    }

    fn stroke_line_thick(&mut self, a: Vec3, b: Vec3, color: [u8; 4], thickness: i32) {
        let Some((x0, y0)) = self.project(a) else {
            return;
        };
        let Some((x1, y1)) = self.project(b) else {
            return;
        };
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x0;
        let mut y = y0;
        loop {
            for oy in -thickness + 1..=thickness - 1 {
                for ox in -thickness + 1..=thickness - 1 {
                    self.put_px(x + ox, y + oy, color);
                }
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn fill_quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [u8; 4]) {
        self.fill_tri(a, b, c, color);
        self.fill_tri(a, c, d, color);
    }

    fn fill_tri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [u8; 4]) {
        let Some((x0, y0)) = self.project(a) else {
            return;
        };
        let Some((x1, y1)) = self.project(b) else {
            return;
        };
        let Some((x2, y2)) = self.project(c) else {
            return;
        };
        let min_x = x0.min(x1).min(x2).max(0);
        let max_x = x0.max(x1).max(x2).min(self.w as i32 - 1);
        let min_y = y0.min(y1).min(y2).max(0);
        let max_y = y0.max(y1).max(y2).min(self.h as i32 - 1);
        let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
        if area == 0 {
            return;
        }
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let w0 = (x1 - x) * (y2 - y) - (x2 - x) * (y1 - y);
                let w1 = (x2 - x) * (y0 - y) - (x0 - x) * (y2 - y);
                let w2 = (x0 - x) * (y1 - y) - (x1 - x) * (y0 - y);
                if (w0 >= 0 && w1 >= 0 && w2 >= 0) || (w0 <= 0 && w1 <= 0 && w2 <= 0) {
                    self.put_px(x, y, color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_asset::load_gltf_mesh;
    use crate::scene::SceneGraph;
    use crate::style::resolve_style;

    #[test]
    fn cpu_preview_applies_optional_tint() {
        let mut color = [200, 100, 50, 255];
        tint_rgba(
            &mut color,
            Some(&crate::scene::MaterialOverride {
                tint: Some([0.5, 1.0, 0.0]),
                ..Default::default()
            }),
        );
        assert_eq!(color, [100, 100, 0, 255]);
    }

    #[test]
    fn shared_coplanar_mesh_edge_is_not_a_style_stroke() {
        let adjacent = [(Vec3::UNIT_Y, true), (Vec3::UNIT_Y, true)];
        assert!(!is_silhouette(&adjacent, 2));
        assert!(!is_crease(&adjacent, 2));
        assert!(is_silhouette(&adjacent[..1], 1));
    }

    #[test]
    fn cpu_preview_draws_glb_triangles() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/hierarchy_textured.glb");
        let mesh = load_gltf_mesh(&path).unwrap();
        let mut rgba = vec![0u8; 64 * 64 * 4];
        let view_proj = Mat4::IDENTITY;
        let mut raster = Raster {
            rgba: &mut rgba,
            w: 64,
            h: 64,
            view_proj: &view_proj,
            seed: 1,
            pixel_scale: 1,
        };
        let world = Mat4::from_cols(
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [-2.5, -1.5, 0.0, 1.0],
        );
        draw_mesh(
            &mut raster,
            &mesh,
            &world,
            Vec3::new(0.0, 0.0, 3.0),
            RenderPassKind::Beauty,
            None,
            None,
        );
        assert!(rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 255));
    }

    #[test]
    fn cpu_renders_demo_png() {
        let backend = CpuWireframeBackend;
        let out = backend
            .render(&RenderRequest {
                scene: SceneGraph::demo_scene(),
                pass: RenderPassKind::Wireframe,
                width: 128,
                height: 96,
                stub_rgb: (0, 0, 0),
                style: None,
                preset: None,
            })
            .expect("cpu render");
        assert_eq!(
            &out.png[0..8],
            &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]
        );
        assert_eq!(out.width, 128);
        assert_eq!(out.backend_id, RenderBackendId::Cpu);
    }

    #[test]
    fn cpu_npr_styles_differ() {
        let backend = CpuWireframeBackend;
        let scene = SceneGraph::demo_scene();
        let mut pngs = Vec::new();
        for id in ["sketch", "pencil", "ink"] {
            let out = backend
                .render(&RenderRequest {
                    scene: scene.clone(),
                    pass: RenderPassKind::Beauty,
                    width: 96,
                    height: 72,
                    stub_rgb: (0, 0, 0),
                    style: Some(resolve_style(id).unwrap()),
                    preset: None,
                })
                .expect("npr");
            pngs.push(out.png);
        }
        assert_ne!(pngs[0], pngs[1]);
        assert_ne!(pngs[1], pngs[2]);
    }

    #[test]
    fn oriented_ground_corners_stay_flat() {
        let scene = SceneGraph::demo_scene();
        let world = scene.world_matrix("ground").expect("ground world");
        let corners = oriented_box_corners(&world);
        let ys: Vec<f32> = corners.iter().map(|c| c.y).collect();
        let y_span = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
            - ys.iter().cloned().fold(f32::INFINITY, f32::min);
        let xs: Vec<f32> = corners.iter().map(|c| c.x).collect();
        let x_span = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
            - xs.iter().cloned().fold(f32::INFINITY, f32::min);
        assert!(
            y_span < 0.2,
            "ground must stay thin in Y (got span {y_span}), not an isotropic cube"
        );
        assert!(
            x_span > 4.0,
            "ground must keep wide X extent (got {x_span})"
        );
    }
}
