//! Generic layer/composition primitives for rich declarative UI (issue #150 lot 5).
//!
//! Host-owned interaction state (drag, undo) stays in the egui renderer; packages
//! store layer JSON in local/document state slots.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;

/// Maximum layers per `layer_canvas` instance.
pub const MAX_LAYERS_PER_CANVAS: u32 = 64;

/// Maximum undo/redo depth per canvas (bounded contract).
pub const MAX_UNDO_DEPTH: u32 = 32;

/// Normalized placement layer (0..1 frame coordinates).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct RichLayer {
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Prompt fragment sent to the image model for this composition block.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
}

fn default_true() -> bool {
    true
}

impl RichLayer {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            x: 0.35,
            y: 0.35,
            w: 0.30,
            h: 0.30,
            label: String::new(),
            prompt: String::new(),
            visible: true,
            locked: false,
        }
    }

    pub fn clamp_in_frame(&mut self) {
        self.w = self.w.clamp(0.05, 1.0);
        self.h = self.h.clamp(0.05, 1.0);
        self.x = self.x.clamp(0.0, 1.0 - self.w);
        self.y = self.y.clamp(0.0, 1.0 - self.h);
    }
}

/// Parse layers from a local-state JSON array value.
pub fn layers_from_value(value: &Value) -> Vec<RichLayer> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Serialize layers back into a JSON array.
pub fn layers_to_value(layers: &[RichLayer]) -> Value {
    Value::Array(
        layers
            .iter()
            .map(|l| serde_json::to_value(l).unwrap_or(Value::Null))
            .collect(),
    )
}

/// Snapshot of canvas-editable state for undo/redo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayerCanvasSnapshot {
    pub layers: Vec<RichLayer>,
    pub selected_id: Option<u64>,
    pub next_id: u64,
}

impl LayerCanvasSnapshot {
    pub fn from_parts(layers: Vec<RichLayer>, selected_id: Option<u64>, next_id: u64) -> Self {
        Self {
            layers,
            selected_id,
            next_id,
        }
    }

    pub fn to_value(&self) -> Value {
        json!({
            "layers": self.layers,
            "selected_id": self.selected_id,
            "next_id": self.next_id,
        })
    }
}

/// Bounded undo stack (host-side, not persisted in package documents).
#[derive(Debug, Clone, Default)]
pub struct BoundedUndoStack {
    undo: VecDeque<LayerCanvasSnapshot>,
    redo: VecDeque<LayerCanvasSnapshot>,
    max_depth: u32,
}

impl BoundedUndoStack {
    pub fn new(max_depth: u32) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            max_depth: max_depth.max(1),
        }
    }

    pub fn push(&mut self, snapshot: LayerCanvasSnapshot) {
        if self.undo.len() >= self.max_depth as usize {
            self.undo.pop_front();
        }
        self.undo.push_back(snapshot);
        self.redo.clear();
    }

    pub fn undo(&mut self, current: LayerCanvasSnapshot) -> Option<LayerCanvasSnapshot> {
        if let Some(prev) = self.undo.pop_back() {
            if self.redo.len() >= self.max_depth as usize {
                self.redo.pop_front();
            }
            self.redo.push_back(current);
            Some(prev)
        } else {
            None
        }
    }

    pub fn redo(&mut self, current: LayerCanvasSnapshot) -> Option<LayerCanvasSnapshot> {
        if let Some(next) = self.redo.pop_back() {
            if self.undo.len() >= self.max_depth as usize {
                self.undo.pop_front();
            }
            self.undo.push_back(current);
            Some(next)
        } else {
            None
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

/// Semantic interaction payload for layer canvas commit/cancel.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct LayerInteractionValue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<RichLayer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
}

pub fn bring_layer_to_front(layers: &mut Vec<RichLayer>, id: u64) {
    if let Some(pos) = layers.iter().position(|l| l.id == id) {
        let layer = layers.remove(pos);
        layers.push(layer);
    }
}

pub fn reorder_layer(layers: &mut Vec<RichLayer>, from: usize, to: usize) {
    if from >= layers.len() || to >= layers.len() || from == to {
        return;
    }
    let layer = layers.remove(from);
    layers.insert(to, layer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_clamps_to_frame() {
        let mut layer = RichLayer {
            id: 1,
            x: 0.9,
            y: 0.9,
            w: 0.5,
            h: 0.5,
            label: String::new(),
            prompt: String::new(),
            visible: true,
            locked: false,
        };
        layer.clamp_in_frame();
        assert!(layer.x + layer.w <= 1.0);
        assert!(layer.y + layer.h <= 1.0);
    }

    #[test]
    fn undo_stack_bounded() {
        let mut stack = BoundedUndoStack::new(2);
        for i in 0..5 {
            stack.push(LayerCanvasSnapshot::from_parts(vec![], Some(i), i + 1));
        }
        assert_eq!(stack.undo.len(), 2);
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut stack = BoundedUndoStack::new(MAX_UNDO_DEPTH);
        let s0 = LayerCanvasSnapshot::from_parts(vec![RichLayer::new(1)], None, 2);
        let s1 = LayerCanvasSnapshot::from_parts(vec![RichLayer::new(2)], Some(2), 3);
        stack.push(s0.clone());
        let restored = stack.undo(s1.clone()).expect("undo");
        assert_eq!(restored, s0);
        let again = stack.redo(s1.clone()).expect("redo");
        assert_eq!(again, s1);
    }

    #[test]
    fn layers_json_round_trip() {
        let layers = vec![RichLayer::new(7)];
        let val = layers_to_value(&layers);
        let parsed = layers_from_value(&val);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, 7);
    }
}
