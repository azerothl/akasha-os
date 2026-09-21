//! CPU SceneGraph wireframe / flat beauty backend (software, ADR 0011).

use super::backend::{
    RenderBackend, RenderBackendId, RenderError, RenderOutput, RenderPassKind, RenderRequest,
};
use crate::math::{Mat4, Vec3};
use crate::png::encode_rgba8_png;
use crate::scene::{NodeKind, SceneGraph};

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
        let proj = perspective_rh(50.0_f32.to_radians(), aspect, 0.1, 200.0);
        let view_proj = proj * view;

        let bg = match req.pass {
            RenderPassKind::Beauty => [28u8, 34, 42, 255],
            RenderPassKind::Wireframe => [18u8, 20, 24, 255],
        };
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for px in rgba.as_chunks_mut::<4>().0 {
            *px = bg;
        }

        // Depth-ish sort: draw farther boxes first for crude beauty fill.
        let mut boxes: Vec<(String, f32)> = Vec::new();
        for id in req.scene.node_ids_depth_first() {
            let Some(node) = req.scene.nodes.get(&id) else {
                continue;
            };
            if !node.visible || node.kind != NodeKind::MeshBox {
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
        };
        for (id, _) in boxes {
            let Ok(world) = req.scene.world_matrix(&id) else {
                continue;
            };
            let center = world.transform_point(Vec3::ZERO);
            let half_v = world.transform_vector(Vec3::new(0.5, 0.5, 0.5));
            let half = half_v.length() * 0.5;
            let half = half.max(0.15);
            let corners = box_corners(center, half);
            let line = match req.pass {
                RenderPassKind::Beauty => [200u8, 180, 140, 255],
                RenderPassKind::Wireframe => [160u8, 200, 220, 255],
            };
            if matches!(req.pass, RenderPassKind::Beauty) {
                raster.fill_quad(
                    corners[0],
                    corners[1],
                    corners[2],
                    corners[3],
                    [70, 78, 90, 255],
                );
            }
            for (i, j) in BOX_EDGES {
                raster.stroke_line(corners[i], corners[j], line);
            }
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

struct Raster<'a> {
    rgba: &'a mut [u8],
    w: u32,
    h: u32,
    view_proj: &'a Mat4,
}

impl Raster<'_> {
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
        self.rgba[i..i + 4].copy_from_slice(&color);
    }

    fn stroke_line(&mut self, a: Vec3, b: Vec3, color: [u8; 4]) {
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
            self.put_px(x, y, color);
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
            })
            .expect("cpu render");
        assert_eq!(&out.png[0..8], &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
        assert_eq!(out.width, 128);
        assert_eq!(out.backend_id, RenderBackendId::Cpu);
    }
}
