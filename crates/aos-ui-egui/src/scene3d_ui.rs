//! Host-local `scene3d` / `scene_tree` DeclUI widgets (Illustration Studio foundation).
//!
//! Pointer orbit / select / TRS stay in the host process (same contract as
//! `layer_canvas`): no WASM round-trip per mouse move. SceneGraph JSON in
//! local state is the source of truth; camera orbit is host-only chrome.

use aos_proto::decl_ui::{DeclUiDocument, DeclUiWidget};
use aos_scene::{
    load_project_yaml, save_project_yaml, Mat4, NodeKind, ProjectFile, SceneGraph, SceneOp,
    Transform, UndoStack, Vec3,
};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Orbit,
    Pan,
    Translate,
}

#[derive(Debug, Clone)]
struct DragState {
    mode: DragMode,
    start: Pos2,
    yaw0: f32,
    pitch0: f32,
    target0: Vec3,
    node_t0: Option<Transform>,
    node_id: Option<String>,
}

#[derive(Debug)]
pub struct Scene3dHostState {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    drag: Option<DragState>,
    undo: UndoStack,
}

impl Default for Scene3dHostState {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.45,
            distance: 6.0,
            target: Vec3::new(0.0, 0.5, 0.0),
            drag: None,
            undo: UndoStack::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Scene3dPatch {
    pub scene_key: String,
    pub scene: Value,
    pub selected_key: String,
    pub selected: Value,
    pub beauty_path_key: Option<String>,
    pub beauty_path: Option<Value>,
}

pub fn patch_to_local_map(patch: &Scene3dPatch) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    m.insert(patch.scene_key.clone(), patch.scene.clone());
    m.insert(patch.selected_key.clone(), patch.selected.clone());
    if let (Some(k), Some(v)) = (&patch.beauty_path_key, &patch.beauty_path) {
        m.insert(k.clone(), v.clone());
    }
    m
}

fn scene_from_local(local: &HashMap<String, Value>, scene_key: &str) -> (SceneGraph, bool) {
    match local.get(scene_key) {
        Some(Value::String(s)) if !s.trim().is_empty() => {
            if let Ok(p) = load_project_yaml(s) {
                return (p.scene, false);
            }
            if let Ok(g) = serde_json::from_str::<SceneGraph>(s) {
                return (g, false);
            }
        }
        Some(v) if !v.is_null() => {
            if let Ok(g) = serde_json::from_value::<SceneGraph>(v.clone()) {
                return (g, false);
            }
        }
        _ => {}
    }
    (SceneGraph::demo_scene(), true)
}

fn scene_to_value(graph: &SceneGraph) -> Value {
    let project = ProjectFile::new(graph.clone());
    match save_project_yaml(&project) {
        Ok(yaml) => Value::String(yaml),
        Err(_) => serde_json::to_value(graph).unwrap_or(Value::Null),
    }
}

fn selected_from_local(local: &HashMap<String, Value>, key: &str) -> Option<String> {
    local
        .get(key)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .filter(|s| !s.is_empty())
}

fn eye_from_orbit(host: &Scene3dHostState) -> Vec3 {
    let cp = host.pitch.cos();
    let offset = Vec3::new(
        host.yaw.sin() * cp,
        host.pitch.sin(),
        host.yaw.cos() * cp,
    ) * host.distance;
    host.target + offset
}

fn project_point(p: Vec3, view_proj: &Mat4, rect: Rect) -> Option<Pos2> {
    let clip = view_proj.transform_point(p);
    // Crude NDC: assume transform_point already divided; treat as camera space projection.
    // We build a simple perspective below that maps to NDC-ish [-1,1].
    if !clip.x.is_finite() || !clip.y.is_finite() {
        return None;
    }
    let x = (clip.x * 0.5 + 0.5) * rect.width() + rect.min.x;
    let y = (1.0 - (clip.y * 0.5 + 0.5)) * rect.height() + rect.min.y;
    Some(Pos2::new(x, y))
}

fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye)
        .normalized()
        .unwrap_or(Vec3::new(0.0, 0.0, -1.0));
    let s = f.cross(up).normalized().unwrap_or(Vec3::UNIT_X);
    let u = s.cross(f);
    // Column-major view matrix (world → camera), camera looks −Z.
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

fn widget_label(w: &DeclUiWidget, doc: &DeclUiDocument, language: &str, fallback: &str) -> String {
    w.label_key
        .as_ref()
        .and_then(|k| doc.labels.as_ref().and_then(|l| l.resolve(language, k)))
        .or_else(|| w.label.clone())
        .unwrap_or_else(|| fallback.into())
}

/// Orbit (LMB empty / Alt+LMB), select (click mesh), translate (drag selected).
pub fn ui_scene3d(
    ui: &mut Ui,
    w: &DeclUiWidget,
    doc: &DeclUiDocument,
    language: &str,
    local_state: &HashMap<String, Value>,
    host: &mut Scene3dHostState,
) -> Option<Scene3dPatch> {
    let scene_key = w.scene_key.as_deref().unwrap_or("scene");
    let selected_key = w.selected_key.as_deref().unwrap_or("selected_id");
    let (mut graph, seeded) = scene_from_local(local_state, scene_key);
    let mut selected = selected_from_local(local_state, selected_key);
    let mut patch: Option<Scene3dPatch> = None;
    if seeded {
        patch = Some(Scene3dPatch {
            scene_key: scene_key.into(),
            scene: scene_to_value(&graph),
            selected_key: selected_key.into(),
            selected: match &selected {
                Some(s) => Value::String(s.clone()),
                None => Value::String("box".into()),
            },
            beauty_path_key: None,
            beauty_path: None,
        });
    }

    let title = widget_label(w, doc, language, "Viewport");
    ui.label(egui::RichText::new(title).strong());

    let desired = Vec2::new(ui.available_width().max(160.0), 280.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    // Atmosphere
    painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 28, 34));
    // Ground grid hint
    let eye = eye_from_orbit(host);
    let aspect = rect.width() / rect.height().max(1.0);
    let view = look_at_rh(eye, host.target, Vec3::UNIT_Y);
    let proj = perspective_rh(50.0_f32.to_radians(), aspect, 0.1, 200.0);
    let view_proj = proj.mul(view);

    // Grid
    for i in -4..=4 {
        let a = Vec3::new(i as f32, 0.0, -4.0);
        let b = Vec3::new(i as f32, 0.0, 4.0);
        let c = Vec3::new(-4.0, 0.0, i as f32);
        let d = Vec3::new(4.0, 0.0, i as f32);
        if let (Some(pa), Some(pb)) = (
            project_point(a, &view_proj, rect),
            project_point(b, &view_proj, rect),
        ) {
            painter.line_segment([pa, pb], Stroke::new(1.0_f32, Color32::from_rgb(48, 56, 64)));
        }
        if let (Some(pc), Some(pd)) = (
            project_point(c, &view_proj, rect),
            project_point(d, &view_proj, rect),
        ) {
            painter.line_segment([pc, pd], Stroke::new(1.0_f32, Color32::from_rgb(48, 56, 64)));
        }
    }

    // Draw mesh boxes in world space
    let mut hit_candidates: Vec<(String, Pos2, f32)> = Vec::new();
    for id in graph.node_ids_depth_first() {
        let Some(node) = graph.nodes.get(&id) else {
            continue;
        };
        if !node.visible || node.kind != NodeKind::MeshBox {
            continue;
        }
        let Ok(world) = graph.world_matrix(&id) else {
            continue;
        };
        let center = world.transform_point(Vec3::ZERO);
        let half = world.transform_vector(Vec3::new(0.5, 0.5, 0.5)).length() * 0.5;
        let half = half.max(0.25);
        let corners = box_corners(center, half);
        let selected_here = selected.as_deref() == Some(id.as_str());
        let stroke = if selected_here {
            Stroke::new(2.0_f32, Color32::from_rgb(120, 200, 255))
        } else {
            Stroke::new(1.5_f32, Color32::from_rgb(180, 160, 120))
        };
        for (i, j) in BOX_EDGES {
            if let (Some(pa), Some(pb)) = (
                project_point(corners[i], &view_proj, rect),
                project_point(corners[j], &view_proj, rect),
            ) {
                painter.line_segment([pa, pb], stroke);
            }
        }
        if let Some(screen) = project_point(center, &view_proj, rect) {
            let depth = (center - eye).length();
            hit_candidates.push((id.clone(), screen, depth));
            painter.circle_filled(
                screen,
                if selected_here { 5.0 } else { 3.0 },
                if selected_here {
                    Color32::from_rgb(120, 200, 255)
                } else {
                    Color32::from_rgb(220, 200, 140)
                },
            );
        }
    }

    // Camera gizmo
    if let Some(cam_id) = graph.active_camera.clone() {
        if let Ok(pos) = graph.world_translation(&cam_id) {
            if let Some(p) = project_point(pos, &view_proj, rect) {
                painter.circle_stroke(p, 6.0, Stroke::new(1.5_f32, Color32::from_rgb(140, 200, 140)));
            }
        }
    }

    let pointer = response.interact_pointer_pos();
    let alt = ui.input(|i| i.modifiers.alt);
    let secondary = response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle);

    if response.drag_started() {
        if let Some(pos) = pointer {
            let mode = if secondary || alt {
                DragMode::Orbit
            } else if selected.is_some() {
                DragMode::Translate
            } else {
                DragMode::Orbit
            };
            let node_id = selected.clone();
            let node_t0 = node_id
                .as_ref()
                .and_then(|id| graph.nodes.get(id).map(|n| n.transform.clone()));
            host.drag = Some(DragState {
                mode,
                start: pos,
                yaw0: host.yaw,
                pitch0: host.pitch,
                target0: host.target,
                node_t0,
                node_id,
            });
        }
    }

    if response.dragged() {
        if let (Some(d), Some(pos)) = (host.drag.as_ref(), pointer) {
            let dx = pos.x - d.start.x;
            let dy = pos.y - d.start.y;
            match d.mode {
                DragMode::Orbit => {
                    host.yaw = d.yaw0 + dx * 0.01;
                    host.pitch = (d.pitch0 + dy * 0.01).clamp(-FRAC_PI_2 + 0.05, FRAC_PI_2 - 0.05);
                }
                DragMode::Pan => {
                    let right = Vec3::new(host.yaw.cos(), 0.0, -host.yaw.sin());
                    let up = Vec3::UNIT_Y;
                    host.target = d.target0 + right * (-dx * 0.01) + up * (dy * 0.01);
                }
                DragMode::Translate => {
                    if let (Some(id), Some(t0)) = (d.node_id.clone(), d.node_t0.clone()) {
                        let right = Vec3::new(host.yaw.cos(), 0.0, -host.yaw.sin());
                        let forward = Vec3::new(host.yaw.sin(), 0.0, host.yaw.cos());
                        let mut after = t0.clone();
                        after.translation =
                            t0.translation + right * (dx * 0.01) + forward * (-dy * 0.01);
                        let before = t0;
                        let _ = host.undo.push_apply(
                            &mut graph,
                            SceneOp::SetTransform {
                                id: id.clone(),
                                before,
                                after,
                            },
                        );
                        patch = Some(Scene3dPatch {
                            scene_key: scene_key.into(),
                            scene: scene_to_value(&graph),
                            selected_key: selected_key.into(),
                            selected: json!(selected),
                            beauty_path_key: None,
                            beauty_path: None,
                        });
                    }
                }
            }
        }
    }

    if response.drag_stopped() {
        host.drag = None;
        // commit already applied via undo ops during drag for translate
    }

    if response.clicked() && !response.dragged() {
        if let Some(pos) = pointer {
            let mut best: Option<(String, f32)> = None;
            for (id, screen, depth) in &hit_candidates {
                let dist = (*screen - pos).length();
                if dist < 14.0 {
                    let score = dist + depth * 0.01;
                    if best.as_ref().map(|(_, s)| score < *s).unwrap_or(true) {
                        best = Some((id.clone(), score));
                    }
                }
            }
            selected = best.map(|(id, _)| id);
            patch = Some(Scene3dPatch {
                scene_key: scene_key.into(),
                scene: scene_to_value(&graph),
                selected_key: selected_key.into(),
                selected: match &selected {
                    Some(s) => Value::String(s.clone()),
                    None => Value::Null,
                },
                beauty_path_key: None,
                beauty_path: None,
            });
        }
    }

    // Scroll zoom — host local
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if response.hovered() && scroll.abs() > 0.0 {
        host.distance = (host.distance * (1.0 - scroll * 0.001)).clamp(1.0, 40.0);
    }

    // TRS numeric strip for selected
    if let Some(id) = selected.clone() {
        if let Some(node) = graph.nodes.get(&id).cloned() {
            ui.horizontal(|ui| {
                ui.label(format!("{} · TRS", node.name));
            });
            let mut t = node.transform.translation;
            let mut changed = false;
            ui.horizontal(|ui| {
                ui.label("T");
                changed |= ui
                    .add(egui::DragValue::new(&mut t.x).speed(0.01).prefix("x "))
                    .changed();
                changed |= ui
                    .add(egui::DragValue::new(&mut t.y).speed(0.01).prefix("y "))
                    .changed();
                changed |= ui
                    .add(egui::DragValue::new(&mut t.z).speed(0.01).prefix("z "))
                    .changed();
            });
            if changed {
                let before = node.transform.clone();
                let mut after = before.clone();
                after.translation = t;
                let _ = host.undo.push_apply(
                    &mut graph,
                    SceneOp::SetTransform {
                        id: id.clone(),
                        before,
                        after,
                    },
                );
                patch = Some(Scene3dPatch {
                    scene_key: scene_key.into(),
                    scene: scene_to_value(&graph),
                    selected_key: selected_key.into(),
                    selected: Value::String(id),
                    beauty_path_key: None,
                    beauty_path: None,
                });
            }
        }
    }

    patch
}

pub fn ui_scene_tree(
    ui: &mut Ui,
    w: &DeclUiWidget,
    doc: &DeclUiDocument,
    language: &str,
    local_state: &HashMap<String, Value>,
) -> Option<Scene3dPatch> {
    let scene_key = w.scene_key.as_deref().unwrap_or("scene");
    let selected_key = w.selected_key.as_deref().unwrap_or("selected_id");
    let (graph, seeded) = scene_from_local(local_state, scene_key);
    let mut selected = selected_from_local(local_state, selected_key);
    let mut patch = if seeded {
        Some(Scene3dPatch {
            scene_key: scene_key.into(),
            scene: scene_to_value(&graph),
            selected_key: selected_key.into(),
            selected: match &selected {
                Some(s) => Value::String(s.clone()),
                None => Value::String("box".into()),
            },
            beauty_path_key: None,
            beauty_path: None,
        })
    } else {
        None
    };

    let title = widget_label(w, doc, language, "Scene");
    ui.label(egui::RichText::new(title).strong());

    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            for id in graph.node_ids_depth_first() {
                let Some(node) = graph.nodes.get(&id) else {
                    continue;
                };
                let depth = {
                    let mut d = 0u32;
                    let mut p = node.parent.clone();
                    while let Some(pid) = p {
                        d += 1;
                        p = graph.nodes.get(&pid).and_then(|n| n.parent.clone());
                    }
                    d
                };
                let label = format!(
                    "{}{} ({})",
                    "  ".repeat(depth as usize),
                    node.name,
                    match node.kind {
                        NodeKind::Empty => "empty",
                        NodeKind::MeshBox => "box",
                        NodeKind::Camera => "camera",
                        NodeKind::Light => "light",
                    }
                );
                let is_sel = selected.as_deref() == Some(id.as_str());
                if ui.selectable_label(is_sel, label).clicked() {
                    selected = Some(id.clone());
                    patch = Some(Scene3dPatch {
                        scene_key: scene_key.into(),
                        scene: scene_to_value(&graph),
                        selected_key: selected_key.into(),
                        selected: Value::String(id),
                        beauty_path_key: None,
                        beauty_path: None,
                    });
                }
            }
        });

    patch
}
