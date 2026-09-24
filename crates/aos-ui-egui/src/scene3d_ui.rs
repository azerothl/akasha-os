//! Host-local `scene3d` / `scene_tree` DeclUI widgets (Illustration Studio).
//!
//! Pointer orbit / select / TRS stay in the host process (same contract as
//! `layer_canvas`): no WASM round-trip per mouse move. SceneGraph in local
//! state is the only source of truth; the wgpu mesh paint is an approximate
//! **edit viewport** (not RenderService beauty / NPR).

use aos_proto::decl_ui::{DeclUiDocument, DeclUiWidget};
use aos_scene::{
    align_nodes, apply_orbit_to_active_camera, duplicate_nodes, embedded_primitives_pack,
    eye_from_orbit as orbit_eye, fovy_from_hfov, group_nodes, instantiate_asset, load_project_yaml,
    look_at_rh, orbit_from_active_camera, perspective_rh, save_project_yaml, snap_nodes, AlignMode,
    EditAxis, LockKind, LockScope, LockTable, Mat4, MaterialOverride, NodeKind, ProjectFile, Quat,
    SceneGraph, SceneOp, Transform, UndoStack, Vec3, ViewportCamera, ViewportRenderer,
};
use eframe::egui::{
    self, Color32, ColorImage, Pos2, Rect, Sense, Stroke, TextureOptions, Ui, Vec2,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const AUTOSAVE_DEBOUNCE: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditTool {
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Orbit,
    Pan,
    Translate,
    Rotate,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GizmoAxis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone)]
struct DragState {
    mode: DragMode,
    axis: Option<GizmoAxis>,
    start: Pos2,
    yaw0: f32,
    pitch0: f32,
    target0: Vec3,
    node_t0: Option<Transform>,
    node_id: Option<String>,
    group_t0: Vec<(String, Transform)>,
}

#[derive(Clone)]
pub enum AssetDrag {
    Prefab(String),
    Mesh { uri: String, name: String },
}

#[derive(Debug)]
pub struct Scene3dHostState {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    /// Horizontal FOV (radians) — mirrors SceneGraph `CameraParams`.
    pub hfov_rad: f32,
    /// Fingerprint of last scene yaml we pulled/pushed (camera sync).
    scene_fp: u64,
    drag: Option<DragState>,
    pub undo: UndoStack,
    pub tool: EditTool,
    pub focused: bool,
    pub selection: Vec<String>,
    pub snap_grid_m: f32,
    pub align_mode: AlignMode,
    pub last_edit_error: Option<String>,
    /// Deferred project autosave after SceneGraph mutations.
    autosave_dirty: bool,
    autosave_due: Option<Instant>,
}

impl Default for Scene3dHostState {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.45,
            distance: 8.0,
            target: Vec3::new(0.4, 0.8, 0.0),
            hfov_rad: aos_scene::hfov_rad(&aos_scene::CameraParams::default()),
            scene_fp: 0,
            drag: None,
            undo: UndoStack::default(),
            tool: EditTool::Translate,
            focused: false,
            selection: Vec::new(),
            snap_grid_m: 0.25,
            align_mode: AlignMode::Center,
            last_edit_error: None,
            autosave_dirty: false,
            autosave_due: None,
        }
    }
}

impl Scene3dHostState {
    pub fn mark_autosave_dirty(&mut self) {
        self.autosave_dirty = true;
        self.autosave_due = Some(Instant::now() + AUTOSAVE_DEBOUNCE);
    }

    pub fn take_autosave_if_due(&mut self) -> bool {
        if !self.autosave_dirty {
            return false;
        }
        match self.autosave_due {
            Some(due) if Instant::now() >= due => {
                self.autosave_dirty = false;
                self.autosave_due = None;
                true
            }
            Some(due) => {
                // Caller should request a repaint before `due`.
                let _ = due;
                false
            }
            None => {
                self.autosave_dirty = false;
                true
            }
        }
    }

    pub fn autosave_remaining(&self) -> Option<Duration> {
        self.autosave_due
            .map(|due| due.saturating_duration_since(Instant::now()))
            .filter(|d| !d.is_zero())
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
    /// When true, host should persist project YAML (debounced autosave fired).
    pub request_autosave: bool,
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
    let (graph, _locks, seeded) = project_from_local(local, scene_key);
    (graph, seeded)
}

fn project_from_local(
    local: &HashMap<String, Value>,
    scene_key: &str,
) -> (SceneGraph, LockTable, bool) {
    match local.get(scene_key) {
        Some(Value::String(s)) if !s.trim().is_empty() => {
            if let Ok(p) = load_project_yaml(s) {
                return (p.scene, p.locks, false);
            }
            if let Ok(g) = serde_json::from_str::<SceneGraph>(s) {
                return (g, LockTable::default(), false);
            }
        }
        Some(v) if !v.is_null() => {
            if let Ok(g) = serde_json::from_value::<SceneGraph>(v.clone()) {
                return (g, LockTable::default(), false);
            }
        }
        _ => {}
    }
    (SceneGraph::demo_scene(), LockTable::default(), true)
}

fn lock_marker(locks: &LockTable, graph: &SceneGraph, id: &str) -> &'static str {
    let covering = locks.list().into_iter().find(|lock| match lock.scope {
        LockScope::Node => lock.node_id == id,
        LockScope::Subtree => {
            if lock.node_id == id {
                return true;
            }
            let mut cur = graph.nodes.get(id).and_then(|n| n.parent.clone());
            while let Some(pid) = cur {
                if pid == lock.node_id {
                    return true;
                }
                cur = graph.nodes.get(&pid).and_then(|n| n.parent.clone());
            }
            false
        }
    });
    match covering.map(|l| l.kind) {
        Some(LockKind::Pose) => " [pose-lock]",
        Some(LockKind::Semantic) => " [locked]",
        None => "",
    }
}

fn scene_to_value(graph: &SceneGraph) -> Value {
    scene_to_value_with_locks(graph, &LockTable::default(), None)
}

fn scene_to_value_with_locks(
    graph: &SceneGraph,
    locks: &LockTable,
    selected_id: Option<String>,
) -> Value {
    let mut project = ProjectFile::new(graph.clone());
    project.locks = locks.clone();
    project.selected_id = selected_id;
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

pub(crate) fn scene_gpu() -> Option<&'static Mutex<ViewportRenderer>> {
    static GPU: OnceLock<Option<Mutex<ViewportRenderer>>> = OnceLock::new();
    GPU.get_or_init(|| ViewportRenderer::new().ok().map(Mutex::new))
        .as_ref()
}

fn eye_from_orbit(host: &Scene3dHostState) -> Vec3 {
    orbit_eye(host.yaw, host.pitch, host.distance, host.target)
}

fn scene_fingerprint(local: &HashMap<String, Value>, scene_key: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    match local.get(scene_key) {
        Some(Value::String(s)) => s.hash(&mut h),
        Some(v) => v.to_string().hash(&mut h),
        None => 0u8.hash(&mut h),
    }
    h.finish()
}

fn pull_orbit_from_scene(host: &mut Scene3dHostState, graph: &SceneGraph) {
    if let Some((yaw, pitch, dist, target, hfov, _)) =
        orbit_from_active_camera(graph, host.distance.max(1.0))
    {
        host.yaw = yaw;
        host.pitch = pitch;
        host.distance = dist.clamp(1.0, 40.0);
        host.target = target;
        host.hfov_rad = hfov;
    }
}

fn push_orbit_to_scene(host: &mut Scene3dHostState, graph: &mut SceneGraph) -> bool {
    let Some(cam_id) = graph.active_camera.clone() else {
        return false;
    };
    let Some(node) = graph.nodes.get(&cam_id).cloned() else {
        return false;
    };
    if node.kind != NodeKind::Camera {
        return false;
    }
    let before = node.transform.clone();
    let before_params = node.camera.clone().unwrap_or_default();
    if apply_orbit_to_active_camera(
        graph,
        host.yaw,
        host.pitch,
        host.distance,
        host.target,
        host.hfov_rad,
    )
    .is_err()
    {
        return false;
    }
    let Some(after_node) = graph.nodes.get(&cam_id).cloned() else {
        return false;
    };
    let after = after_node.transform.clone();
    let after_params = after_node.camera.clone().unwrap_or_default();
    if before == after && before_params == after_params {
        return false;
    }
    host.undo.push_applied(SceneOp::SetCamera {
        id: cam_id,
        before,
        after,
        before_params,
        after_params,
    });
    true
}

fn camera_strip_labels(language: &str) -> (&'static str, &'static str, &'static str, &'static str) {
    if language.starts_with("fr") {
        ("Caméra", "Viser", "Distance", "FOV")
    } else {
        ("Camera", "Look-at", "Distance", "FOV")
    }
}

fn project_point(p: Vec3, view_proj: &Mat4, rect: Rect) -> Option<Pos2> {
    let clip = view_proj.transform_point(p);
    if !clip.x.is_finite() || !clip.y.is_finite() {
        return None;
    }
    let x = (clip.x * 0.5 + 0.5) * rect.width() + rect.min.x;
    let y = (1.0 - (clip.y * 0.5 + 0.5)) * rect.height() + rect.min.y;
    Some(Pos2::new(x, y))
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

fn paint_cpu_fallback(
    painter: &egui::Painter,
    rect: Rect,
    graph: &SceneGraph,
    selected: Option<&str>,
    view_proj: &Mat4,
    eye: Vec3,
) -> (Vec<(String, Pos2, f32)>, u32) {
    painter.rect_filled(rect, 4.0, Color32::from_rgb(24, 28, 34));
    for i in -4..=4 {
        let a = Vec3::new(i as f32, 0.0, -4.0);
        let b = Vec3::new(i as f32, 0.0, 4.0);
        let c = Vec3::new(-4.0, 0.0, i as f32);
        let d = Vec3::new(4.0, 0.0, i as f32);
        if let (Some(pa), Some(pb)) = (
            project_point(a, view_proj, rect),
            project_point(b, view_proj, rect),
        ) {
            painter.line_segment(
                [pa, pb],
                Stroke::new(1.0_f32, Color32::from_rgb(48, 56, 64)),
            );
        }
        if let (Some(pc), Some(pd)) = (
            project_point(c, view_proj, rect),
            project_point(d, view_proj, rect),
        ) {
            painter.line_segment(
                [pc, pd],
                Stroke::new(1.0_f32, Color32::from_rgb(48, 56, 64)),
            );
        }
    }

    let mut hit_candidates: Vec<(String, Pos2, f32)> = Vec::new();
    let mut mesh_count = 0u32;
    for id in graph.node_ids_depth_first() {
        let Some(node) = graph.nodes.get(&id) else {
            continue;
        };
        if !node.visible || !matches!(node.kind, NodeKind::MeshBox | NodeKind::MeshAsset) {
            continue;
        }
        mesh_count += 1;
        let Ok(world) = graph.world_matrix(&id) else {
            continue;
        };
        let center = world.transform_point(Vec3::ZERO);
        let half = world.transform_vector(Vec3::new(0.5, 0.5, 0.5)).length() * 0.5;
        let half = half.max(0.25);
        let corners = box_corners(center, half);
        let selected_here = selected == Some(id.as_str());
        let stroke = if selected_here {
            Stroke::new(2.0_f32, Color32::from_rgb(120, 200, 255))
        } else if node.kind == NodeKind::MeshAsset {
            Stroke::new(1.5_f32, Color32::from_rgb(90, 180, 170))
        } else {
            Stroke::new(1.5_f32, Color32::from_rgb(180, 160, 120))
        };
        for (i, j) in BOX_EDGES {
            if let (Some(pa), Some(pb)) = (
                project_point(corners[i], view_proj, rect),
                project_point(corners[j], view_proj, rect),
            ) {
                painter.line_segment([pa, pb], stroke);
            }
        }
        if let Some(screen) = project_point(center, view_proj, rect) {
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
    (hit_candidates, mesh_count)
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
    let (mut graph, locks, seeded) = project_from_local(local_state, scene_key);
    let mut selected = selected_from_local(local_state, selected_key);
    host.selection.retain(|id| graph.nodes.contains_key(id));
    if host.selection.is_empty() {
        if let Some(id) = selected.as_ref().filter(|id| graph.nodes.contains_key(*id)) {
            host.selection.push(id.clone());
        }
    }
    let scene_val =
        |g: &SceneGraph, sel: &Option<String>| scene_to_value_with_locks(g, &locks, sel.clone());
    let mut patch: Option<Scene3dPatch> = None;
    if seeded {
        patch = Some(Scene3dPatch {
            scene_key: scene_key.into(),
            scene: scene_val(&graph, &selected),
            selected_key: selected_key.into(),
            selected: match &selected {
                Some(s) => Value::String(s.clone()),
                None => Value::String("box".into()),
            },
            beauty_path_key: None,
            beauty_path: None,
            request_autosave: false,
        });
    }

    // Pull orbit from SceneGraph when the project yaml changes (load / compose).
    let fp = scene_fingerprint(local_state, scene_key);
    if fp != host.scene_fp || seeded {
        pull_orbit_from_scene(host, &graph);
        host.scene_fp = fp;
    }

    let title = widget_label(w, doc, language, "Viewport");
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong());
        ui.label(
            egui::RichText::new(if language.starts_with("fr") {
                "édition · pas beauté"
            } else {
                "edit view · not beauty"
            })
            .weak()
            .small(),
        );
    });

    // Prefer DeclUI `size` (px); otherwise fill the stage pane, leaving headroom
    // for TRS / camera chrome below the viewport.
    let avail_h = ui.available_height();
    let viewport_h = w.size.map(|s| s.clamp(160.0, 1200.0)).unwrap_or_else(|| {
        if avail_h.is_finite() && avail_h > 320.0 {
            (avail_h - 160.0).clamp(220.0, 720.0)
        } else if avail_h.is_finite() && avail_h > 1.0 {
            (avail_h * 0.55).clamp(200.0, 720.0)
        } else {
            280.0
        }
    });
    let desired = Vec2::new(ui.available_width().max(160.0), viewport_h);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    let eye = eye_from_orbit(host);
    let aspect = rect.width() / rect.height().max(1.0);
    let fovy = fovy_from_hfov(host.hfov_rad, aspect);
    let view = look_at_rh(eye, host.target, Vec3::UNIT_Y);
    let proj = perspective_rh(fovy, aspect, 0.1, 200.0);
    let view_proj = proj * view;

    let mut hit_candidates: Vec<(String, Pos2, f32)> = Vec::new();
    let mut mesh_count = 0u32;
    let mut used_wgpu = false;

    if let Some(gpu) = scene_gpu() {
        if let Ok(gpu) = gpu.lock() {
            let px_w = (rect.width() * ui.ctx().pixels_per_point())
                .round()
                .clamp(64.0, 2048.0) as u32;
            let px_h = (rect.height() * ui.ctx().pixels_per_point())
                .round()
                .clamp(64.0, 2048.0) as u32;
            let cam = ViewportCamera {
                eye,
                target: host.target,
                up: Vec3::UNIT_Y,
                fovy_rad: fovy,
                near: 0.1,
                far: 200.0,
            };
            if let Ok(rgba) = gpu.render_rgba(&graph, &cam, px_w, px_h, selected.as_deref()) {
                let image =
                    ColorImage::from_rgba_unmultiplied([px_w as usize, px_h as usize], &rgba);
                let tex = ui.ctx().load_texture(
                    "illustration_scene3d_wgpu",
                    image,
                    TextureOptions::LINEAR,
                );
                painter.image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
                used_wgpu = true;
                for id in graph.node_ids_depth_first() {
                    let Some(node) = graph.nodes.get(&id) else {
                        continue;
                    };
                    if !node.visible
                        || !matches!(node.kind, NodeKind::MeshBox | NodeKind::MeshAsset)
                    {
                        continue;
                    }
                    mesh_count += 1;
                    let Ok(world) = graph.world_matrix(&id) else {
                        continue;
                    };
                    let center = world.transform_point(Vec3::ZERO);
                    if let Some(screen) = project_point(center, &view_proj, rect) {
                        let depth = (center - eye).length();
                        hit_candidates.push((id.clone(), screen, depth));
                        let selected_here = selected.as_deref() == Some(id.as_str());
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
            }
        }
    }

    if !used_wgpu {
        let (hits, count) =
            paint_cpu_fallback(&painter, rect, &graph, selected.as_deref(), &view_proj, eye);
        hit_candidates = hits;
        mesh_count = count;
    }

    if mesh_count == 0 {
        let empty = w
            .empty_label_key
            .as_ref()
            .and_then(|key| doc.labels.as_ref().and_then(|l| l.resolve(language, key)))
            .unwrap_or_else(|| "No scene meshes".into());
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            empty,
            egui::FontId::proportional(14.0),
            Color32::from_rgb(160, 168, 176),
        );
    }

    // Camera marker
    if let Some(cam_id) = graph.active_camera.clone() {
        if let Ok(pos) = graph.world_translation(&cam_id) {
            if let Some(p) = project_point(pos, &view_proj, rect) {
                painter.circle_stroke(
                    p,
                    6.0,
                    Stroke::new(1.5_f32, Color32::from_rgb(140, 200, 140)),
                );
            }
        }
    }

    // TRS axis gizmos for selected non-camera node
    let mut gizmo_hits: Vec<(GizmoAxis, Pos2, f32)> = Vec::new();
    if let Some(id) = selected.as_ref() {
        if graph
            .nodes
            .get(id)
            .map(|n| n.kind != NodeKind::Camera)
            .unwrap_or(false)
        {
            if let Ok(origin) = graph.world_translation(id) {
                let axis_len = (host.distance * 0.12).clamp(0.35, 1.8);
                let axes = [
                    (GizmoAxis::X, Vec3::UNIT_X, Color32::from_rgb(220, 80, 80)),
                    (GizmoAxis::Y, Vec3::UNIT_Y, Color32::from_rgb(80, 200, 100)),
                    (GizmoAxis::Z, Vec3::UNIT_Z, Color32::from_rgb(80, 140, 230)),
                ];
                if let Some(o) = project_point(origin, &view_proj, rect) {
                    for (axis, dir, color) in axes {
                        let tip = origin + dir * axis_len;
                        if let Some(t) = project_point(tip, &view_proj, rect) {
                            let stroke = Stroke::new(
                                if host.tool == EditTool::Translate {
                                    2.5_f32
                                } else {
                                    2.0_f32
                                },
                                color,
                            );
                            painter.line_segment([o, t], stroke);
                            match host.tool {
                                EditTool::Translate => {
                                    painter.circle_filled(t, 4.5, color);
                                }
                                EditTool::Rotate => {
                                    painter.circle_stroke(t, 6.0, Stroke::new(1.5_f32, color));
                                }
                                EditTool::Scale => {
                                    let r = egui::Rect::from_center_size(t, egui::vec2(8.0, 8.0));
                                    painter.rect_filled(r, 1.0, color);
                                }
                            }
                            gizmo_hits.push((axis, t, (tip - eye).length()));
                        }
                    }
                }
            }
        }
    }

    if response.hovered() || response.dragged() || response.clicked() {
        host.focused = true;
    }

    let pointer = response.interact_pointer_pos();
    let alt = ui.input(|i| i.modifiers.alt);
    let secondary = response.dragged_by(egui::PointerButton::Secondary)
        || response.dragged_by(egui::PointerButton::Middle);

    if response.drag_started() {
        if let Some(pos) = pointer {
            let mut axis_pick: Option<GizmoAxis> = None;
            let mut best = 14.0_f32;
            for (axis, screen, _) in &gizmo_hits {
                let dist = (*screen - pos).length();
                if dist < best {
                    best = dist;
                    axis_pick = Some(*axis);
                }
            }
            let mode = if secondary || alt {
                DragMode::Orbit
            } else if selected.is_some() {
                match host.tool {
                    EditTool::Translate => DragMode::Translate,
                    EditTool::Rotate => DragMode::Rotate,
                    EditTool::Scale => DragMode::Scale,
                }
            } else {
                DragMode::Orbit
            };
            let node_id = selected.clone();
            let node_t0 = node_id
                .as_ref()
                .and_then(|id| graph.nodes.get(id).map(|n| n.transform.clone()));
            let group_t0 = host
                .selection
                .iter()
                .filter_map(|id| {
                    graph
                        .nodes
                        .get(id)
                        .map(|n| (id.clone(), n.transform.clone()))
                })
                .collect();
            host.drag = Some(DragState {
                mode,
                axis: axis_pick,
                start: pos,
                yaw0: host.yaw,
                pitch0: host.pitch,
                target0: host.target,
                node_t0,
                node_id,
                group_t0,
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
                DragMode::Translate | DragMode::Rotate | DragMode::Scale => {
                    if let (Some(id), Some(t0)) = (d.node_id.clone(), d.node_t0.clone()) {
                        let after = apply_tool_drag(d.mode, d.axis, &t0, dx, dy, host.yaw);
                        let _ = graph.set_transform(&id, after.clone());
                        for (other_id, original) in &d.group_t0 {
                            if other_id == &id {
                                continue;
                            }
                            let transformed =
                                apply_tool_drag(d.mode, d.axis, original, dx, dy, host.yaw);
                            let _ = graph.set_transform(other_id, transformed);
                        }
                        patch = Some(Scene3dPatch {
                            scene_key: scene_key.into(),
                            scene: scene_val(&graph, &selected),
                            selected_key: selected_key.into(),
                            selected: json!(selected),
                            beauty_path_key: None,
                            beauty_path: None,
                            request_autosave: false,
                        });
                    }
                }
            }
        }
    }

    if response.drag_stopped() {
        let drag = host.drag.take();
        if let Some(d) = drag {
            match d.mode {
                DragMode::Orbit | DragMode::Pan => {
                    if push_orbit_to_scene(host, &mut graph) {
                        let yaml = scene_val(&graph, &selected);
                        host.scene_fp = fingerprint_value(&yaml);
                        host.mark_autosave_dirty();
                        patch = Some(Scene3dPatch {
                            scene_key: scene_key.into(),
                            scene: yaml,
                            selected_key: selected_key.into(),
                            selected: json!(selected),
                            beauty_path_key: None,
                            beauty_path: None,
                            request_autosave: false,
                        });
                    }
                }
                DragMode::Translate | DragMode::Rotate | DragMode::Scale => {
                    if let (Some(id), Some(before)) = (d.node_id, d.node_t0) {
                        if let Some(after) = graph.nodes.get(&id).map(|n| n.transform.clone()) {
                            if after != before {
                                let mut ops = vec![SceneOp::SetTransform {
                                    id: id.clone(),
                                    before,
                                    after,
                                }];
                                for (other_id, original) in d.group_t0 {
                                    if other_id == id {
                                        continue;
                                    }
                                    if let Some(updated) =
                                        graph.nodes.get(&other_id).map(|n| n.transform.clone())
                                    {
                                        if updated != original {
                                            ops.push(SceneOp::SetTransform {
                                                id: other_id,
                                                before: original,
                                                after: updated,
                                            });
                                        }
                                    }
                                }
                                host.undo.push_applied(if ops.len() == 1 {
                                    ops.remove(0)
                                } else {
                                    SceneOp::Batch { ops }
                                });
                                host.mark_autosave_dirty();
                                patch = Some(Scene3dPatch {
                                    scene_key: scene_key.into(),
                                    scene: scene_val(&graph, &selected),
                                    selected_key: selected_key.into(),
                                    selected: json!(selected),
                                    beauty_path_key: None,
                                    beauty_path: None,
                                    request_autosave: false,
                                });
                            }
                        }
                    }
                }
            }
        }
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
            if ui.input(|i| i.modifiers.shift) {
                if let Some(id) = &selected {
                    if host.selection.contains(id) {
                        host.selection.retain(|old| old != id);
                    } else if host.selection.len() < 32 {
                        host.selection.push(id.clone());
                    }
                }
            } else {
                host.selection = selected.clone().into_iter().collect();
            }
            selected = host.selection.last().cloned();
            patch = Some(Scene3dPatch {
                scene_key: scene_key.into(),
                scene: scene_val(&graph, &selected),
                selected_key: selected_key.into(),
                selected: match &selected {
                    Some(s) => Value::String(s.clone()),
                    None => Value::Null,
                },
                beauty_path_key: None,
                beauty_path: None,
                request_autosave: false,
            });
        }
    }

    if let Some(payload) = response.dnd_release_payload::<AssetDrag>() {
        let before = graph.clone();
        let dropped_id = match payload.as_ref() {
            AssetDrag::Prefab(asset_id) => embedded_primitives_pack()
                .ok()
                .and_then(|pack| {
                    instantiate_asset(&mut graph, &pack, asset_id, Some("root"), "drop_").ok()
                })
                .map(|instance| instance.root_id),
            AssetDrag::Mesh { uri, name } => (1..=10000)
                .map(|n| format!("mesh_drop_{n}"))
                .find(|id| !graph.nodes.contains_key(id))
                .and_then(|id| {
                    aos_scene::insert_mesh_asset(
                        &mut graph,
                        "root",
                        &id,
                        name.clone(),
                        uri,
                        Transform::default(),
                    )
                    .ok()
                    .map(|_| id)
                }),
        };
        if let Some(root_id) = dropped_id {
            let pointer = ui.input(|i| i.pointer.interact_pos());
            let position = pointer
                .and_then(|p| ground_drop_position(p, rect, eye, host.target, host.hfov_rad, fovy))
                .unwrap_or(host.target);
            if let Some(node) = graph.nodes.get_mut(&root_id) {
                node.transform.translation = Vec3::new(position.x, 0.0, position.z);
            }
            if graph.validate().is_ok() {
                selected = Some(root_id.clone());
                host.selection = vec![root_id];
                host.undo.push_applied(SceneOp::ReplaceGraph {
                    before: Box::new(before),
                    after: Box::new(graph.clone()),
                });
                host.mark_autosave_dirty();
                patch = Some(Scene3dPatch {
                    scene_key: scene_key.into(),
                    scene: scene_val(&graph, &selected),
                    selected_key: selected_key.into(),
                    selected: json!(selected),
                    beauty_path_key: None,
                    beauty_path: None,
                    request_autosave: false,
                });
            }
        }
    }

    // Scroll zoom — host local; persist camera on change
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if response.hovered() && scroll.abs() > 0.0 {
        host.distance = (host.distance * (1.0 - scroll * 0.001)).clamp(1.0, 40.0);
        if push_orbit_to_scene(host, &mut graph) {
            let yaml = scene_val(&graph, &selected);
            host.scene_fp = fingerprint_value(&yaml);
            host.mark_autosave_dirty();
            patch = Some(Scene3dPatch {
                scene_key: scene_key.into(),
                scene: yaml,
                selected_key: selected_key.into(),
                selected: json!(selected),
                beauty_path_key: None,
                beauty_path: None,
                request_autosave: false,
            });
        }
    }

    // Tool + undo/redo chrome (global edit chrome)
    let fr = language.starts_with("fr");
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(if fr { "Outil" } else { "Tool" }).strong());
        for (tool, en, fr_l) in [
            (EditTool::Translate, "Move", "Déplacer"),
            (EditTool::Rotate, "Rotate", "Rotation"),
            (EditTool::Scale, "Scale", "Échelle"),
        ] {
            let label = if fr { fr_l } else { en };
            if ui.selectable_label(host.tool == tool, label).clicked() {
                host.tool = tool;
            }
        }
        ui.separator();
        let undo_l = if fr { "Annuler" } else { "Undo" };
        let redo_l = if fr { "Rétablir" } else { "Redo" };
        if ui
            .add_enabled(host.undo.can_undo(), egui::Button::new(undo_l))
            .clicked()
            && host.undo.undo(&mut graph).unwrap_or(false)
        {
            host.mark_autosave_dirty();
            patch = Some(Scene3dPatch {
                scene_key: scene_key.into(),
                scene: scene_val(&graph, &selected),
                selected_key: selected_key.into(),
                selected: json!(selected),
                beauty_path_key: None,
                beauty_path: None,
                request_autosave: false,
            });
        }
        if ui
            .add_enabled(host.undo.can_redo(), egui::Button::new(redo_l))
            .clicked()
            && host.undo.redo(&mut graph).unwrap_or(false)
        {
            host.mark_autosave_dirty();
            patch = Some(Scene3dPatch {
                scene_key: scene_key.into(),
                scene: scene_val(&graph, &selected),
                selected_key: selected_key.into(),
                selected: json!(selected),
                beauty_path_key: None,
                beauty_path: None,
                request_autosave: false,
            });
        }
    });

    // Cmd/Ctrl+Z / Shift+Z / Y when viewport focused
    if host.focused {
        let do_undo =
            ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z) && !i.modifiers.shift);
        let do_redo = ui.input(|i| {
            (i.modifiers.command && i.key_pressed(egui::Key::Y))
                || (i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::Z))
        });
        let applied = if do_undo {
            host.undo.undo(&mut graph).unwrap_or(false)
        } else if do_redo {
            host.undo.redo(&mut graph).unwrap_or(false)
        } else {
            false
        };
        if applied {
            host.mark_autosave_dirty();
            patch = Some(Scene3dPatch {
                scene_key: scene_key.into(),
                scene: scene_val(&graph, &selected),
                selected_key: selected_key.into(),
                selected: json!(selected),
                beauty_path_key: None,
                beauty_path: None,
                request_autosave: false,
            });
        }
    }

    // Camera strip (orbit + look-at + FOV) — always for active camera.
    if graph.active_camera.is_some() {
        let (cam_l, look_l, dist_l, fov_l) = camera_strip_labels(language);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(cam_l).strong());
        });
        let mut changed = false;
        let mut look = host.target;
        ui.horizontal_wrapped(|ui| {
            ui.label(look_l);
            changed |= ui
                .add(egui::DragValue::new(&mut look.x).speed(0.01).prefix("x "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut look.y).speed(0.01).prefix("y "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut look.z).speed(0.01).prefix("z "))
                .changed();
        });
        let mut dist = host.distance;
        let mut yaw_deg = host.yaw.to_degrees();
        let mut pitch_deg = host.pitch.to_degrees();
        let mut fov_deg = host.hfov_rad.to_degrees();
        ui.horizontal_wrapped(|ui| {
            ui.label(dist_l);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut dist)
                        .speed(0.05)
                        .range(1.0..=40.0),
                )
                .changed();
            ui.label(if fr { "Orbite" } else { "Orbit" });
            changed |= ui
                .add(egui::DragValue::new(&mut yaw_deg).speed(0.5).suffix("° y"))
                .changed();
            changed |= ui
                .add(
                    egui::DragValue::new(&mut pitch_deg)
                        .speed(0.5)
                        .suffix("° p")
                        .range(-85.0..=85.0),
                )
                .changed();
            ui.label(fov_l);
            changed |= ui
                .add(
                    egui::DragValue::new(&mut fov_deg)
                        .speed(0.5)
                        .suffix("°")
                        .range(15.0..=120.0),
                )
                .changed();
        });
        if changed {
            host.target = look;
            host.distance = dist.clamp(1.0, 40.0);
            host.yaw = yaw_deg.to_radians();
            host.pitch = pitch_deg
                .to_radians()
                .clamp(-FRAC_PI_2 + 0.05, FRAC_PI_2 - 0.05);
            host.hfov_rad = fov_deg.to_radians();
            if push_orbit_to_scene(host, &mut graph) {
                let yaml = scene_val(&graph, &selected);
                host.scene_fp = fingerprint_value(&yaml);
                host.mark_autosave_dirty();
                patch = Some(Scene3dPatch {
                    scene_key: scene_key.into(),
                    scene: yaml,
                    selected_key: selected_key.into(),
                    selected: json!(selected),
                    beauty_path_key: None,
                    beauty_path: None,
                    request_autosave: false,
                });
            }
        }
    }

    // Full TRS numeric strip for selected (non-camera nodes)
    if let Some(id) = selected.clone() {
        if let Some(node) = graph.nodes.get(&id).cloned() {
            if node.kind != NodeKind::Camera {
                ui.horizontal(|ui| {
                    ui.label(format!("{} · TRS", node.name));
                    ui.label(
                        egui::RichText::new(if fr {
                            "T mètres · R ° XYZ · S"
                        } else {
                            "T metres · R ° XYZ · S"
                        })
                        .weak()
                        .small(),
                    );
                });
                let mut t = node.transform.translation;
                let (rx0, ry0, rz0) = node.transform.rotation.to_euler_xyz();
                let mut r_deg = [rx0.to_degrees(), ry0.to_degrees(), rz0.to_degrees()];
                let mut s = node.transform.scale;
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
                ui.horizontal(|ui| {
                    ui.label("R");
                    changed |= ui
                        .add(egui::DragValue::new(&mut r_deg[0]).speed(0.5).suffix("° x"))
                        .changed();
                    changed |= ui
                        .add(egui::DragValue::new(&mut r_deg[1]).speed(0.5).suffix("° y"))
                        .changed();
                    changed |= ui
                        .add(egui::DragValue::new(&mut r_deg[2]).speed(0.5).suffix("° z"))
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label("S");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut s.x)
                                .speed(0.01)
                                .prefix("x ")
                                .range(0.01..=100.0),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut s.y)
                                .speed(0.01)
                                .prefix("y ")
                                .range(0.01..=100.0),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut s.z)
                                .speed(0.01)
                                .prefix("z ")
                                .range(0.01..=100.0),
                        )
                        .changed();
                });
                if changed {
                    let before = node.transform.clone();
                    let mut after = before.clone();
                    after.translation = t;
                    after.rotation = Quat::from_euler_xyz(
                        r_deg[0].to_radians(),
                        r_deg[1].to_radians(),
                        r_deg[2].to_radians(),
                    );
                    after.scale = s;
                    let _ = host.undo.push_apply(
                        &mut graph,
                        SceneOp::SetTransform {
                            id: id.clone(),
                            before,
                            after,
                        },
                    );
                    host.mark_autosave_dirty();
                    patch = Some(Scene3dPatch {
                        scene_key: scene_key.into(),
                        scene: scene_val(&graph, &selected),
                        selected_key: selected_key.into(),
                        selected: Value::String(id),
                        beauty_path_key: None,
                        beauty_path: None,
                        request_autosave: false,
                    });
                }
            }
        }
    }

    // Deferred autosave tick
    if let Some(rem) = host.autosave_remaining() {
        ui.ctx().request_repaint_after(rem);
    }
    if host.take_autosave_if_due() {
        let yaml = scene_val(&graph, &selected);
        let mut p = patch.unwrap_or_else(|| Scene3dPatch {
            scene_key: scene_key.into(),
            scene: yaml.clone(),
            selected_key: selected_key.into(),
            selected: json!(selected),
            beauty_path_key: None,
            beauty_path: None,
            request_autosave: true,
        });
        p.scene = yaml;
        p.request_autosave = true;
        patch = Some(p);
    }

    patch
}

fn ground_drop_position(
    pointer: Pos2,
    rect: Rect,
    eye: Vec3,
    target: Vec3,
    hfov: f32,
    fovy: f32,
) -> Option<Vec3> {
    let forward = (target - eye).normalized()?;
    let right = forward.cross(Vec3::UNIT_Y).normalized()?;
    let up = right.cross(forward).normalized()?;
    let nx = (pointer.x - rect.center().x) / (rect.width() * 0.5);
    let ny = (rect.center().y - pointer.y) / (rect.height() * 0.5);
    let ray = (forward + right * (nx * (hfov * 0.5).tan()) + up * (ny * (fovy * 0.5).tan()))
        .normalized()?;
    if ray.y >= -0.001 {
        return None;
    }
    let distance = -eye.y / ray.y;
    let point = eye + ray * distance;
    if !point.is_finite() || point.x.abs() > 20.0 || point.z.abs() > 20.0 {
        return None;
    }
    Some(point)
}

pub fn ui_scene_asset_palette(ui: &mut Ui, language: &str) {
    let fr = language.starts_with("fr");
    ui.label(
        egui::RichText::new(if fr {
            "Glisser vers la vue 3D"
        } else {
            "Drag into the 3D view"
        })
        .weak()
        .small(),
    );
    let items = [
        ("prop.box", "Box", "Boîte"),
        ("prop.chair", "Chair", "Chaise"),
        ("prop.sofa", "Sofa", "Canapé"),
        ("prop.desk", "Desk", "Bureau"),
        ("prop.table", "Table", "Table"),
        ("prop.lamp", "Lamp", "Lampe"),
        ("prop.book", "Book", "Livre"),
        ("prop.plant", "Plant", "Plante"),
        ("humanoid.placeholder", "Person", "Personne"),
    ];
    ui.horizontal_wrapped(|ui| {
        for (id, en, french) in items {
            let label = if fr { french } else { en };
            ui.dnd_drag_source(ui.id().with(id), AssetDrag::Prefab(id.into()), |ui| {
                ui.label(egui::RichText::new(label).strong());
            });
        }
    });
    let storage =
        crate::os_open::aos_home().join("var/storage/data/documents/illustrations/assets");
    let imported = storage.join("imported");
    if let Ok(files) = std::fs::read_dir(&imported) {
        for entry in files.flatten().take(50) {
            let path = entry.path();
            let Some(filename) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(hash) = filename.strip_suffix(".metadata.json") else {
                continue;
            };
            if hash.len() != 16
                || !hash.bytes().all(|c| c.is_ascii_hexdigit())
                || !imported.join(format!("{hash}.glb")).is_file()
            {
                continue;
            }
            let Ok(raw) = std::fs::read(&path) else {
                continue;
            };
            let Ok(meta) = serde_json::from_slice::<Value>(&raw) else {
                continue;
            };
            let name = meta
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Imported GLB")
                .to_owned();
            let uri = format!("/documents/illustrations/assets/imported/{hash}.glb");
            ui.dnd_drag_source(
                ui.id().with(&uri),
                AssetDrag::Mesh {
                    uri,
                    name: name.clone(),
                },
                |ui| {
                    ui.label(format!("GLB · {name}"));
                },
            );
        }
    }
    for id in ["WoodenChair_01", "SchoolDesk_01", "binder_notebook"] {
        let path = storage.join("catalogue").join(id).join("1k");
        if !path.join("source.gltf").is_file() {
            continue;
        }
        let name = std::fs::read(path.join("metadata.json"))
            .ok()
            .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
            .and_then(|meta| meta.get("name").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_else(|| id.into());
        let uri = format!("/documents/illustrations/assets/catalogue/{id}/1k/source.gltf");
        ui.dnd_drag_source(
            ui.id().with(&uri),
            AssetDrag::Mesh {
                uri,
                name: name.clone(),
            },
            |ui| {
                ui.label(format!("CC0 · {name}"));
            },
        );
    }
}

pub fn ui_scene_object_tools(
    ui: &mut Ui,
    w: &DeclUiWidget,
    language: &str,
    local_state: &HashMap<String, Value>,
    host: &mut Scene3dHostState,
) -> Option<Scene3dPatch> {
    let scene_key = w.scene_key.as_deref().unwrap_or("scene");
    let selected_key = w.selected_key.as_deref().unwrap_or("selected_id");
    let (mut graph, locks, _) = project_from_local(local_state, scene_key);
    host.selection.retain(|id| graph.nodes.contains_key(id));
    if host.selection.is_empty() {
        if let Some(id) =
            selected_from_local(local_state, selected_key).filter(|id| graph.nodes.contains_key(id))
        {
            host.selection.push(id);
        }
    }
    let fr = language.starts_with("fr");
    ui.label(if fr {
        format!(
            "{} objet(s) sélectionné(s) · Maj + clic pour plusieurs",
            host.selection.len()
        )
    } else {
        format!(
            "{} selected · Shift-click to select multiple",
            host.selection.len()
        )
    });
    enum Command {
        Duplicate,
        Group,
        Snap,
        Align(EditAxis),
    }
    let mut command = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !host.selection.is_empty(),
                egui::Button::new(if fr { "Dupliquer" } else { "Duplicate" }),
            )
            .clicked()
        {
            command = Some(Command::Duplicate);
        }
        if ui
            .add_enabled(
                host.selection.len() > 1,
                egui::Button::new(if fr { "Grouper" } else { "Group" }),
            )
            .clicked()
        {
            command = Some(Command::Group);
        }
        ui.label(if fr { "Pas (m)" } else { "Grid (m)" });
        ui.add(
            egui::DragValue::new(&mut host.snap_grid_m)
                .speed(0.01)
                .range(0.01..=10.0),
        );
        if ui
            .add_enabled(
                !host.selection.is_empty(),
                egui::Button::new(if fr { "Accrocher" } else { "Snap" }),
            )
            .clicked()
        {
            command = Some(Command::Snap);
        }
    });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("scene_align_mode")
            .selected_text(match (fr, host.align_mode) {
                (true, AlignMode::Min) => "Min",
                (true, AlignMode::Center) => "Centre",
                (true, AlignMode::Max) => "Max",
                (false, AlignMode::Min) => "Min",
                (false, AlignMode::Center) => "Center",
                (false, AlignMode::Max) => "Max",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut host.align_mode, AlignMode::Min, "Min");
                ui.selectable_value(
                    &mut host.align_mode,
                    AlignMode::Center,
                    if fr { "Centre" } else { "Center" },
                );
                ui.selectable_value(&mut host.align_mode, AlignMode::Max, "Max");
            });
        for (axis, label) in [(EditAxis::X, "X"), (EditAxis::Y, "Y"), (EditAxis::Z, "Z")] {
            if ui
                .add_enabled(
                    host.selection.len() > 1,
                    egui::Button::new(format!("{} {label}", if fr { "Aligner" } else { "Align" })),
                )
                .clicked()
            {
                command = Some(Command::Align(axis));
            }
        }
    });
    if let Some(error) = &host.last_edit_error {
        ui.colored_label(Color32::LIGHT_RED, error);
    }
    let command = command?;
    let before = graph.clone();
    let result = match command {
        Command::Duplicate => {
            duplicate_nodes(&mut graph, &host.selection).map(|ids| host.selection = ids)
        }
        Command::Group => {
            group_nodes(&mut graph, &host.selection).map(|id| host.selection = vec![id])
        }
        Command::Snap => snap_nodes(&mut graph, &host.selection, host.snap_grid_m),
        Command::Align(axis) => align_nodes(&mut graph, &host.selection, axis, host.align_mode),
    };
    if let Err(error) = result {
        host.last_edit_error = Some(error.to_string());
        return None;
    }
    host.last_edit_error = None;
    let selected = host.selection.first().cloned();
    host.undo.push_applied(SceneOp::ReplaceGraph {
        before: Box::new(before),
        after: Box::new(graph.clone()),
    });
    host.mark_autosave_dirty();
    Some(Scene3dPatch {
        scene_key: scene_key.into(),
        scene: scene_to_value_with_locks(&graph, &locks, selected.clone()),
        selected_key: selected_key.into(),
        selected: json!(selected),
        beauty_path_key: None,
        beauty_path: None,
        request_autosave: false,
    })
}

pub fn ui_scene_material_editor(
    ui: &mut Ui,
    w: &DeclUiWidget,
    language: &str,
    local_state: &HashMap<String, Value>,
    host: &mut Scene3dHostState,
) -> Option<Scene3dPatch> {
    let scene_key = w.scene_key.as_deref().unwrap_or("scene");
    let selected_key = w.selected_key.as_deref().unwrap_or("selected_id");
    let (mut graph, locks, _) = project_from_local(local_state, scene_key);
    let fr = language.starts_with("fr");
    let Some(id) = selected_from_local(local_state, selected_key) else {
        ui.weak(if fr {
            "Sélectionnez un objet pour régler sa matière."
        } else {
            "Select an object to edit its material."
        });
        return None;
    };
    let node = graph.nodes.get(&id)?;
    if !matches!(node.kind, NodeKind::MeshBox | NodeKind::MeshAsset) {
        ui.weak(if fr {
            "Sélectionnez un objet visible."
        } else {
            "Select a visible object."
        });
        return None;
    }
    ui.label(egui::RichText::new(&node.name).strong());
    ui.weak(if fr {
        "Les GLB conservent leurs matériaux source."
    } else {
        "Source GLB materials are preserved."
    });
    let before = node.material.clone();
    let mut after = before.clone();
    let mut edited = false;
    ui.horizontal_wrapped(|ui| {
        if ui.button(if fr { "Source" } else { "Original" }).clicked() {
            after = None;
            edited = true;
        }
        for (label, rgb) in [
            (if fr { "Ivoire" } else { "Ivory" }, [1.0, 0.92, 0.78]),
            (if fr { "Argile" } else { "Clay" }, [0.82, 0.48, 0.34]),
            (if fr { "Bleu" } else { "Blue" }, [0.36, 0.58, 0.95]),
            (if fr { "Encre" } else { "Ink" }, [0.22, 0.27, 0.35]),
        ] {
            if ui.button(label).clicked() {
                after.get_or_insert_with(MaterialOverride::default).tint = Some(rgb);
                edited = true;
            }
        }
    });
    let mut tint = after
        .as_ref()
        .and_then(|m| m.tint)
        .unwrap_or([1.0, 1.0, 1.0]);
    if ui.color_edit_button_rgb(&mut tint).changed() {
        after.get_or_insert_with(MaterialOverride::default).tint = Some(tint);
        edited = true;
    }
    ui.horizontal_wrapped(|ui| {
        for (label, roughness, metallic) in [
            (if fr { "Mat" } else { "Matte" }, 0.9, 0.0),
            ("Satin", 0.45, 0.0),
            (if fr { "Métal" } else { "Metal" }, 0.24, 0.9),
        ] {
            if ui.button(label).clicked() {
                let material = after.get_or_insert_with(MaterialOverride::default);
                material.roughness = Some(roughness);
                material.metallic = Some(metallic);
                edited = true;
            }
        }
    });
    let mut roughness = after.as_ref().and_then(|m| m.roughness).unwrap_or(0.65);
    let mut metallic = after.as_ref().and_then(|m| m.metallic).unwrap_or(0.0);
    if ui
        .add(egui::Slider::new(&mut roughness, 0.0..=1.0).text(if fr {
            "Rugosité"
        } else {
            "Roughness"
        }))
        .changed()
    {
        after
            .get_or_insert_with(MaterialOverride::default)
            .roughness = Some(roughness);
        edited = true;
    }
    if ui
        .add(egui::Slider::new(&mut metallic, 0.0..=1.0).text(if fr {
            "Métallique"
        } else {
            "Metallic"
        }))
        .changed()
    {
        after.get_or_insert_with(MaterialOverride::default).metallic = Some(metallic);
        edited = true;
    }
    if !edited || before == after {
        return None;
    }
    let op = SceneOp::SetMaterial {
        id: id.clone(),
        before,
        after,
    };
    if host.undo.push_apply(&mut graph, op).is_err() {
        return None;
    }
    host.mark_autosave_dirty();
    Some(Scene3dPatch {
        scene_key: scene_key.into(),
        scene: scene_to_value_with_locks(&graph, &locks, Some(id.clone())),
        selected_key: selected_key.into(),
        selected: Value::String(id),
        beauty_path_key: None,
        beauty_path: None,
        request_autosave: false,
    })
}

fn fingerprint_value(v: &Value) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    match v {
        Value::String(s) => s.hash(&mut h),
        other => other.to_string().hash(&mut h),
    }
    h.finish()
}

fn apply_tool_drag(
    mode: DragMode,
    axis: Option<GizmoAxis>,
    t0: &Transform,
    dx: f32,
    dy: f32,
    yaw: f32,
) -> Transform {
    let mut after = t0.clone();
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let forward = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    match mode {
        DragMode::Translate => {
            let delta = match axis {
                Some(GizmoAxis::X) => Vec3::UNIT_X * (dx * 0.01),
                Some(GizmoAxis::Y) => Vec3::UNIT_Y * (-dy * 0.01),
                Some(GizmoAxis::Z) => Vec3::UNIT_Z * (dx * 0.01),
                None => right * (dx * 0.01) + forward * (-dy * 0.01),
            };
            after.translation = t0.translation + delta;
        }
        DragMode::Rotate => {
            let ang = (dx * 0.01) + (dy * 0.01);
            let q = match axis {
                Some(GizmoAxis::X) => Quat::from_axis_angle(Vec3::UNIT_X, ang),
                Some(GizmoAxis::Z) => Quat::from_axis_angle(Vec3::UNIT_Z, ang),
                Some(GizmoAxis::Y) | None => Quat::from_axis_angle(Vec3::UNIT_Y, ang),
            };
            after.rotation = (q * t0.rotation).normalized().unwrap_or(t0.rotation);
        }
        DragMode::Scale => {
            let factor = (1.0 + (dx - dy) * 0.005).clamp(0.05, 20.0);
            match axis {
                Some(GizmoAxis::X) => after.scale.x = (t0.scale.x * factor).clamp(0.01, 100.0),
                Some(GizmoAxis::Y) => after.scale.y = (t0.scale.y * factor).clamp(0.01, 100.0),
                Some(GizmoAxis::Z) => after.scale.z = (t0.scale.z * factor).clamp(0.01, 100.0),
                None => {
                    after.scale = Vec3::new(
                        (t0.scale.x * factor).clamp(0.01, 100.0),
                        (t0.scale.y * factor).clamp(0.01, 100.0),
                        (t0.scale.z * factor).clamp(0.01, 100.0),
                    );
                }
            }
        }
        DragMode::Orbit | DragMode::Pan => {}
    }
    after
}

/// DeclUI `undo_redo` chrome wired to a `scene3d` viewport undo stack.
pub fn ui_scene_undo_redo(
    ui: &mut Ui,
    w: &DeclUiWidget,
    doc: &DeclUiDocument,
    language: &str,
    local_state: &HashMap<String, Value>,
    host: &mut Scene3dHostState,
) -> Option<Scene3dPatch> {
    let scene_key = w.scene_key.as_deref().unwrap_or("scene");
    let selected_key = w.selected_key.as_deref().unwrap_or("selected_id");
    let (mut graph, locks, _) = project_from_local(local_state, scene_key);
    let selected = selected_from_local(local_state, selected_key);
    let fr = language.starts_with("fr");
    let undo_l = doc
        .labels
        .as_ref()
        .and_then(|l| l.resolve(language, "undo_label"))
        .unwrap_or_else(|| if fr { "Annuler".into() } else { "Undo".into() });
    let redo_l = doc
        .labels
        .as_ref()
        .and_then(|l| l.resolve(language, "redo_label"))
        .unwrap_or_else(|| {
            if fr {
                "Rétablir".into()
            } else {
                "Redo".into()
            }
        });

    let mut patch = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(host.undo.can_undo(), egui::Button::new(undo_l))
            .clicked()
            && host.undo.undo(&mut graph).unwrap_or(false)
        {
            host.mark_autosave_dirty();
            patch = Some(Scene3dPatch {
                scene_key: scene_key.into(),
                scene: scene_to_value_with_locks(&graph, &locks, selected.clone()),
                selected_key: selected_key.into(),
                selected: json!(selected),
                beauty_path_key: None,
                beauty_path: None,
                request_autosave: false,
            });
        }
        if ui
            .add_enabled(host.undo.can_redo(), egui::Button::new(redo_l))
            .clicked()
            && host.undo.redo(&mut graph).unwrap_or(false)
        {
            host.mark_autosave_dirty();
            patch = Some(Scene3dPatch {
                scene_key: scene_key.into(),
                scene: scene_to_value_with_locks(&graph, &locks, selected.clone()),
                selected_key: selected_key.into(),
                selected: json!(selected),
                beauty_path_key: None,
                beauty_path: None,
                request_autosave: false,
            });
        }
        if host.autosave_dirty {
            ui.label(
                egui::RichText::new(if fr { "Enregistrement…" } else { "Saving…" })
                    .weak()
                    .small(),
            );
        }
    });
    if let Some(rem) = host.autosave_remaining() {
        ui.ctx().request_repaint_after(rem);
    }
    if host.take_autosave_if_due() {
        let yaml = scene_to_value_with_locks(&graph, &locks, selected.clone());
        let mut p = patch.unwrap_or_else(|| Scene3dPatch {
            scene_key: scene_key.into(),
            scene: yaml.clone(),
            selected_key: selected_key.into(),
            selected: json!(selected),
            beauty_path_key: None,
            beauty_path: None,
            request_autosave: true,
        });
        p.scene = yaml;
        p.request_autosave = true;
        patch = Some(p);
    }
    patch
}

pub fn ui_scene_tree(
    ui: &mut Ui,
    w: &DeclUiWidget,
    doc: &DeclUiDocument,
    language: &str,
    local_state: &HashMap<String, Value>,
    host: &mut Scene3dHostState,
) -> Option<Scene3dPatch> {
    let scene_key = w.scene_key.as_deref().unwrap_or("scene");
    let selected_key = w.selected_key.as_deref().unwrap_or("selected_id");
    let (graph, locks, seeded) = project_from_local(local_state, scene_key);
    let mut selected = selected_from_local(local_state, selected_key);
    host.selection.retain(|id| graph.nodes.contains_key(id));
    if host.selection.is_empty() {
        if let Some(id) = selected.as_ref().filter(|id| graph.nodes.contains_key(*id)) {
            host.selection.push(id.clone());
        }
    }
    let mut patch = if seeded {
        Some(Scene3dPatch {
            scene_key: scene_key.into(),
            scene: scene_to_value_with_locks(&graph, &locks, selected.clone()),
            selected_key: selected_key.into(),
            selected: match &selected {
                Some(s) => Value::String(s.clone()),
                None => Value::String("box".into()),
            },
            beauty_path_key: None,
            beauty_path: None,
            request_autosave: false,
        })
    } else {
        None
    };

    let title = widget_label(w, doc, language, "Scene");
    ui.label(egui::RichText::new(title).strong());

    let node_ids = graph.node_ids_depth_first();
    if node_ids.is_empty() {
        let empty = w
            .empty_label_key
            .as_ref()
            .and_then(|k| doc.labels.as_ref().and_then(|l| l.resolve(language, k)))
            .unwrap_or_else(|| "No scene loaded yet.".into());
        ui.label(egui::RichText::new(empty).weak());
        return patch;
    }

    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            for id in node_ids {
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
                    "{}{} ({}){}",
                    "  ".repeat(depth as usize),
                    node.name,
                    match node.kind {
                        NodeKind::Empty => "empty",
                        NodeKind::MeshBox => "box",
                        NodeKind::MeshAsset => "mesh",
                        NodeKind::Camera => "camera",
                        NodeKind::Light => "light",
                    },
                    lock_marker(&locks, &graph, &id)
                );
                let is_sel = host.selection.iter().any(|s| s == &id);
                if ui.selectable_label(is_sel, label).clicked() {
                    if ui.input(|i| i.modifiers.shift) {
                        if host.selection.contains(&id) {
                            host.selection.retain(|old| old != &id);
                        } else if host.selection.len() < 32 {
                            host.selection.push(id.clone());
                        }
                    } else {
                        host.selection = vec![id.clone()];
                    }
                    selected = host.selection.last().cloned();
                    patch = Some(Scene3dPatch {
                        scene_key: scene_key.into(),
                        scene: scene_to_value_with_locks(&graph, &locks, selected.clone()),
                        selected_key: selected_key.into(),
                        selected: json!(selected),
                        beauty_path_key: None,
                        beauty_path: None,
                        request_autosave: false,
                    });
                }
            }
        });

    patch
}

#[cfg(test)]
mod object_edit_tests {
    use super::*;

    #[test]
    fn center_drop_intersects_ground_at_camera_target() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 500.0));
        let target = Vec3::ZERO;
        let hit = ground_drop_position(
            rect.center(),
            rect,
            Vec3::new(0.0, 2.0, 5.0),
            target,
            1.0,
            0.8,
        )
        .unwrap();
        assert!(hit.x.abs() < 1e-4 && hit.y.abs() < 1e-4 && hit.z.abs() < 1e-4);
    }
}
