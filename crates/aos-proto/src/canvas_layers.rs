//! Named canvas layers / groups and in-place object edits.

use super::{
    canvas_op_bbox, CanvasBBox, CanvasDoc, CanvasOp, CanvasOpBody, CanvasPenStyle, CanvasPoint,
    CANVAS_LAYOUT_MARGIN,
};
use serde::{Deserialize, Serialize};

pub const DEFAULT_CANVAS_LAYER_ID: &str = "lyr-1";

pub fn default_canvas_layer_id() -> String {
    DEFAULT_CANVAS_LAYER_ID.into()
}

fn default_true() -> bool {
    true
}

fn default_opacity() -> f32 {
    1.0
}

/// Named stack entry. A group is a layer with children (`parent_id`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasLayer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

impl Default for CanvasLayer {
    fn default() -> Self {
        Self {
            id: default_canvas_layer_id(),
            name: "Layer 1".into(),
            parent_id: None,
            visible: true,
            locked: false,
            opacity: 1.0,
        }
    }
}

/// In-place document mutation (not appended as a paint op).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum CanvasEdit {
    Delete {
        seq: u64,
    },
    Move {
        seq: u64,
        dx: f32,
        dy: f32,
    },
    Reorder {
        seq: u64,
        /// Target index in `ops` (0 = back).
        z: i64,
    },
    Restyle {
        seq: u64,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        width: Option<f32>,
        #[serde(default)]
        fill: Option<bool>,
        #[serde(default)]
        rotation: Option<f32>,
        #[serde(default)]
        opacity: Option<f32>,
        #[serde(default)]
        dash: Option<Vec<f32>>,
        #[serde(default)]
        gradient: Option<Option<crate::CanvasLinearGradient>>,
    },
    Rotate {
        seq: u64,
        rotation: f32,
    },
    Align {
        seq: u64,
        #[serde(default)]
        to_seq: Option<u64>,
        edges: Vec<String>,
    },
    LayerCreate {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        parent_id: Option<String>,
    },
    LayerRename {
        id: String,
        name: String,
    },
    LayerSet {
        id: String,
        #[serde(default)]
        visible: Option<bool>,
        #[serde(default)]
        locked: Option<bool>,
        #[serde(default)]
        opacity: Option<f32>,
    },
    LayerReorder {
        id: String,
        #[serde(default)]
        parent_id: Option<String>,
        z: i64,
    },
    LayerDelete {
        id: String,
    },
    LayerActivate {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEditRequest {
    pub session_id: String,
    #[serde(default)]
    pub author_id: String,
    #[serde(flatten)]
    pub edit: CanvasEdit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEditResponse {
    pub canvas_open: bool,
    pub next_seq: u64,
    pub ops: Vec<CanvasOp>,
    #[serde(default)]
    pub layers: Vec<CanvasLayer>,
    #[serde(default)]
    pub active_layer_id: String,
    #[serde(default)]
    pub pen: CanvasPenStyle,
}

/// Fill missing layer table so legacy `canvas.json` remains readable.
pub fn ensure_canvas_layers(doc: &mut CanvasDoc) {
    if doc.layers.is_empty() {
        doc.layers.push(CanvasLayer::default());
        if doc.next_layer_id < 2 {
            doc.next_layer_id = 2;
        }
    }
    if doc.active_layer_id.is_empty()
        || !doc
            .layers
            .iter()
            .any(|layer| layer.id == doc.active_layer_id)
    {
        doc.active_layer_id = doc.layers[0].id.clone();
    }
    let fallback = doc.layers[0].id.clone();
    for op in &mut doc.ops {
        if op.layer_id.is_empty() || !doc.layers.iter().any(|layer| layer.id == op.layer_id) {
            op.layer_id = fallback.clone();
        }
    }
}

pub fn canvas_layer_by_id<'a>(doc: &'a CanvasDoc, id: &str) -> Option<&'a CanvasLayer> {
    doc.layers.iter().find(|layer| layer.id == id)
}

fn walk_ancestors<'a>(doc: &'a CanvasDoc, layer_id: &str) -> Vec<&'a CanvasLayer> {
    let mut out = Vec::new();
    let mut id = Some(layer_id);
    let mut guard = 0;
    while let Some(cur) = id {
        if guard > 32 {
            break;
        }
        guard += 1;
        let Some(layer) = canvas_layer_by_id(doc, cur) else {
            break;
        };
        out.push(layer);
        id = layer.parent_id.as_deref();
    }
    out
}

pub fn canvas_layer_effective_visible(doc: &CanvasDoc, layer_id: &str) -> bool {
    walk_ancestors(doc, layer_id)
        .iter()
        .all(|layer| layer.visible)
}

pub fn canvas_layer_effective_locked(doc: &CanvasDoc, layer_id: &str) -> bool {
    walk_ancestors(doc, layer_id)
        .iter()
        .any(|layer| layer.locked)
}

pub fn canvas_layer_effective_opacity(doc: &CanvasDoc, layer_id: &str) -> f32 {
    if !canvas_layer_effective_visible(doc, layer_id) {
        return 0.0;
    }
    walk_ancestors(doc, layer_id)
        .iter()
        .fold(1.0_f32, |acc, layer| acc * layer.opacity.clamp(0.0, 1.0))
}

pub fn translate_canvas_op_body(body: &mut CanvasOpBody, dx: f32, dy: f32) {
    let shift_pt = |p: &mut CanvasPoint| {
        p.x = (p.x + dx).clamp(0.0, 1.0);
        p.y = (p.y + dy).clamp(0.0, 1.0);
    };
    match body {
        CanvasOpBody::Stroke { points, .. }
        | CanvasOpBody::Erase { points, .. }
        | CanvasOpBody::Spline { points, .. }
        | CanvasOpBody::Path { points, .. } => {
            for p in points {
                shift_pt(p);
            }
        }
        CanvasOpBody::Line { p0, p1, .. } => {
            shift_pt(p0);
            shift_pt(p1);
        }
        CanvasOpBody::Rect { x, y, w, h, .. } | CanvasOpBody::Ellipse { x, y, w, h, .. } => {
            *x = (*x + dx).clamp(0.0, 1.0);
            *y = (*y + dy).clamp(0.0, 1.0);
            if *x + *w > 1.0 {
                *w = (1.0 - *x).max(0.01);
            }
            if *y + *h > 1.0 {
                *h = (1.0 - *y).max(0.01);
            }
        }
        CanvasOpBody::Fill { x, y, .. } => {
            *x = (*x + dx).clamp(0.0, 1.0);
            *y = (*y + dy).clamp(0.0, 1.0);
        }
        CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
}

pub fn canvas_rotate_point(cx: f32, cy: f32, x: f32, y: f32, degrees: f32) -> (f32, f32) {
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = x - cx;
    let dy = y - cy;
    (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
}

pub fn canvas_rect_corners(x: f32, y: f32, w: f32, h: f32, rotation: f32) -> [(f32, f32); 4] {
    let pts = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    if rotation.abs() < 0.001 {
        return pts;
    }
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    pts.map(|(px, py)| canvas_rotate_point(cx, cy, px, py, rotation))
}

pub fn set_canvas_op_rotation(body: &mut CanvasOpBody, rotation: f32) -> Result<(), String> {
    match body {
        CanvasOpBody::Rect { rotation: slot, .. }
        | CanvasOpBody::Ellipse { rotation: slot, .. } => {
            *slot = rotation;
            Ok(())
        }
        _ => Err("rotation seulement rect/ellipse".into()),
    }
}

pub fn usable_canvas_bbox() -> CanvasBBox {
    CanvasBBox {
        x0: CANVAS_LAYOUT_MARGIN,
        y0: CANVAS_LAYOUT_MARGIN,
        x1: 1.0 - CANVAS_LAYOUT_MARGIN,
        y1: 1.0 - CANVAS_LAYOUT_MARGIN,
    }
}

pub fn align_canvas_op_body(
    body: &mut CanvasOpBody,
    src: CanvasBBox,
    target: CanvasBBox,
    edges: &[String],
) {
    let mut dx = 0.0;
    let mut dy = 0.0;
    for edge in edges {
        match edge.as_str() {
            "left" => dx = target.x0 - src.x0,
            "right" => dx = target.x1 - src.x1,
            "top" => dy = target.y0 - src.y0,
            "bottom" => dy = target.y1 - src.y1,
            "center_x" => dx = (target.x0 + target.x1) * 0.5 - (src.x0 + src.x1) * 0.5,
            "center_y" => dy = (target.y0 + target.y1) * 0.5 - (src.y0 + src.y1) * 0.5,
            _ => {}
        }
    }
    translate_canvas_op_body(body, dx, dy);
}

pub fn canvas_hit_test<'a, I>(ops: I, x: f32, y: f32) -> Option<u64>
where
    I: IntoIterator<Item = &'a CanvasOp>,
    I::IntoIter: DoubleEndedIterator,
{
    for op in ops.into_iter().rev() {
        if let Some(b) = canvas_op_bbox(&op.body) {
            if x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1 {
                return Some(op.seq);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CanvasOpBody, CanvasPenStyle};

    fn doc_with_layers() -> CanvasDoc {
        let mut doc = CanvasDoc {
            session_id: "s".into(),
            next_seq: 1,
            ops: Vec::new(),
            pen: CanvasPenStyle::default(),
            ..Default::default()
        };
        ensure_canvas_layers(&mut doc);
        doc.layers.push(CanvasLayer {
            id: "lyr-2".into(),
            name: "Roof".into(),
            parent_id: Some("lyr-1".into()),
            visible: true,
            locked: false,
            opacity: 0.5,
        });
        doc.next_layer_id = 3;
        doc
    }

    #[test]
    fn parent_hide_hides_child() {
        let mut doc = doc_with_layers();
        doc.layers[0].visible = false;
        assert!(!canvas_layer_effective_visible(&doc, "lyr-2"));
        assert_eq!(canvas_layer_effective_opacity(&doc, "lyr-2"), 0.0);
    }

    #[test]
    fn opacity_multiplies() {
        let mut doc = doc_with_layers();
        doc.layers[0].opacity = 0.5;
        assert!((canvas_layer_effective_opacity(&doc, "lyr-2") - 0.25).abs() < 1e-6);
    }

    #[test]
    fn parent_lock_locks_child() {
        let mut doc = doc_with_layers();
        doc.layers[0].locked = true;
        assert!(canvas_layer_effective_locked(&doc, "lyr-2"));
    }

    #[test]
    fn translate_clamps_rect() {
        let mut body = CanvasOpBody::Rect {
            x: 0.9,
            y: 0.9,
            w: 0.2,
            h: 0.2,
            color: "#fff".into(),
            fill: true,
            width: 0.01,
            rotation: 0.0,
            opacity: 1.0,
            dash: vec![],
            gradient: None,
        };
        translate_canvas_op_body(&mut body, 0.2, 0.0);
        match body {
            CanvasOpBody::Rect { x, w, .. } => {
                assert!((x - 1.0).abs() < 1e-5);
                assert!(w <= 0.01 + 1e-5);
            }
            _ => panic!("rect"),
        }
    }

    #[test]
    fn ensure_stamps_legacy_ops() {
        let mut doc = CanvasDoc {
            session_id: "s".into(),
            next_seq: 2,
            ops: vec![CanvasOp {
                seq: 1,
                author_id: "human".into(),
                ts_ms: 1,
                layer_id: String::new(),
                body: CanvasOpBody::Clear,
            }],
            pen: CanvasPenStyle::default(),
            ..Default::default()
        };
        ensure_canvas_layers(&mut doc);
        assert_eq!(doc.ops[0].layer_id, DEFAULT_CANVAS_LAYER_ID);
        assert_eq!(doc.layers.len(), 1);
    }

    #[test]
    fn align_left_moves_to_usable_margin() {
        let mut body = CanvasOpBody::Rect {
            x: 0.40,
            y: 0.40,
            w: 0.20,
            h: 0.10,
            color: "#fff".into(),
            fill: true,
            width: 0.01,
            rotation: 0.0,
            opacity: 1.0,
            dash: vec![],
            gradient: None,
        };
        let src = crate::canvas_op_bbox(&body).expect("bbox");
        align_canvas_op_body(&mut body, src, usable_canvas_bbox(), &["left".into()]);
        match body {
            CanvasOpBody::Rect { x, .. } => assert!((x - 0.10).abs() < 1e-4),
            _ => panic!("rect"),
        }
    }

    #[test]
    fn rotated_rect_bbox_is_larger_than_axis_aligned() {
        let axis = crate::canvas_op_bbox(&CanvasOpBody::Rect {
            x: 0.40,
            y: 0.40,
            w: 0.20,
            h: 0.10,
            color: "#fff".into(),
            fill: true,
            width: 0.01,
            rotation: 0.0,
            opacity: 1.0,
            dash: vec![],
            gradient: None,
        })
        .unwrap();
        let rotated = crate::canvas_op_bbox(&CanvasOpBody::Rect {
            x: 0.40,
            y: 0.40,
            w: 0.20,
            h: 0.10,
            color: "#fff".into(),
            fill: true,
            width: 0.01,
            rotation: 45.0,
            opacity: 1.0,
            dash: vec![],
            gradient: None,
        })
        .unwrap();
        assert!(rotated.x1 - rotated.x0 > axis.x1 - axis.x0);
        assert!(rotated.y1 - rotated.y0 > axis.y1 - axis.y0);
    }
}
