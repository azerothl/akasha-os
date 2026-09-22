//! CPU SceneGraph wireframe / flat beauty / NPR approximation backend.
//!
//! When `RenderRequest.style` is set (Sketch / Pencil / Ink), draws a paper
//! background with style-aware line art and optional hatching. Without a style,
//! keeps the legacy dark wireframe / flat beauty look.

use super::backend::{
    RenderBackend, RenderBackendId, RenderError, RenderOutput, RenderPassKind, RenderRequest,
};
use crate::camera::{fovy_from_hfov, hfov_rad};
use crate::light::{collect_lights, linear_to_srgb_u8, shade_diffuse, ResolvedLight};
use crate::math::{Mat4, Vec3};
use crate::png::encode_rgba8_png;
use crate::scene::{NodeKind, SceneGraph};
use crate::style::{ResolvedStyle, StyleFamily};

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
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for px in rgba.as_chunks_mut::<4>().0 {
            *px = bg;
        }
        if let Some(st) = style {
            apply_paper_grain(&mut rgba, w, h, st);
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
            w,
            h,
            view_proj: &view_proj,
            seed: 0xA05C_E11Eu64,
        };
        let lights = collect_lights(&req.scene);
        for (id, _) in boxes {
            let Ok(world) = req.scene.world_matrix(&id) else {
                continue;
            };
            let center = world.transform_point(Vec3::ZERO);
            let half_v = world.transform_vector(Vec3::new(0.5, 0.5, 0.5));
            let half = (half_v.length() * 0.5).max(0.15);
            let corners = box_corners(center, half);
            draw_box(&mut raster, &corners, req.pass, style, &lights);
        }

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
            let n = hash_u32(x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263)));
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
) {
    let Some(st) = style else {
        let line = match pass {
            RenderPassKind::Beauty => [200u8, 180, 140, 255],
            RenderPassKind::Wireframe => [160u8, 200, 220, 255],
        };
        if matches!(pass, RenderPassKind::Beauty) {
            let fill = lit_face_rgba(
                lights,
                corners[0],
                corners[1],
                corners[2],
                [70.0 / 255.0, 78.0 / 255.0, 90.0 / 255.0],
                [70, 78, 90, 255],
            );
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
    let fill_base = {
        let rgb = st.fill_rgb();
        [rgb[0], rgb[1], rgb[2], 255]
    };

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
            lights,
            corners[0],
            corners[1],
            corners[2],
            albedo,
            fill_base,
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
    let thickness = match st.family {
        StyleFamily::Sketch => 1,
        StyleFamily::Pencil => if st.line_width >= 1.5 { 2 } else { 1 },
        StyleFamily::Ink => if st.line_width >= 1.5 { 2 } else { 1 },
    };
    for _ in 0..passes {
        for (i, j) in BOX_EDGES {
            let a = jitter_point(corners[i], st.jitter, raster.next_f32());
            let b = jitter_point(corners[j], st.jitter, raster.next_f32());
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

fn hatch_face(
    raster: &mut Raster<'_>,
    a: Vec3,
    b: Vec3,
    c: Vec3,
    d: Vec3,
    style: &ResolvedStyle,
) {
    let rgb = style.stroke_rgb();
    let alpha = ((0.35 + style.contrast * 0.4) * 255.0) as u8;
    let col = [rgb[0], rgb[1], rgb[2], alpha];
    let steps = match style.family {
        StyleFamily::Sketch => 0,
        StyleFamily::Pencil => 6,
        StyleFamily::Ink => 4,
    };
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
    if let Some(cam_id) = scene.active_camera.as_deref() {
        if let Ok(eye) = scene.world_translation(cam_id) {
            let forward = scene
                .camera_forward_world(cam_id)
                .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
            let target = eye + forward * 4.0;
            return (eye, target);
        }
    }
    (Vec3::new(0.0, 1.5, 4.0), Vec3::new(0.0, 0.5, 0.0))
}

fn camera_projection(scene: &SceneGraph, aspect: f32) -> (f32, f32, f32) {
    if let Some(cam_id) = scene.active_camera.as_deref() {
        if let Some(node) = scene.nodes.get(cam_id) {
            if let Some(params) = node.camera.as_ref() {
                let hfov = hfov_rad(params);
                let fovy = fovy_from_hfov(hfov, aspect);
                return (fovy, params.near.max(0.01), params.far.max(params.near + 1.0));
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

fn box_corners(center: Vec3, half: f32) -> [Vec3; 8] {
    let h = half;
    [
        center + Vec3::new(-h, -h, -h),
        center + Vec3::new(h, -h, -h),
        center + Vec3::new(h, h, -h),
        center + Vec3::new(-h, h, -h),
        center + Vec3::new(-h, -h, h),
        center + Vec3::new(h, -h, h),
        center + Vec3::new(h, h, h),
        center + Vec3::new(-h, h, h),
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
}

impl Raster<'_> {
    fn next_f32(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
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
        self.stroke_line_thick(a, b, color, 1);
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
    use crate::scene::SceneGraph;
    use crate::style::resolve_style;

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
            })
            .expect("cpu render");
        assert_eq!(&out.png[0..8], &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
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
                })
                .expect("npr");
            pngs.push(out.png);
        }
        assert_ne!(pngs[0], pngs[1]);
        assert_ne!(pngs[1], pngs[2]);
    }
}
