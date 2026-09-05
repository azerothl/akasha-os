//! Per-op paint style: opacity, dashes, linear gradients.

use serde::{Deserialize, Serialize};

use crate::{
    normalize_canvas_color, CanvasAspect, CanvasDoc, CanvasLayer, CanvasOp, CanvasOpBody,
    CanvasPenStyle,
};

/// Linear fill gradient in normalized board space (angle in degrees, 0 = left→right).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanvasLinearGradient {
    pub color0: String,
    pub color1: String,
    #[serde(default)]
    pub angle_deg: f32,
}

pub fn default_canvas_opacity() -> f32 {
    1.0
}

pub fn canvas_op_body_opacity(body: &CanvasOpBody) -> f32 {
    match body {
        CanvasOpBody::Stroke { opacity, .. }
        | CanvasOpBody::Rect { opacity, .. }
        | CanvasOpBody::Ellipse { opacity, .. }
        | CanvasOpBody::Line { opacity, .. }
        | CanvasOpBody::Spline { opacity, .. }
        | CanvasOpBody::Path { opacity, .. }
        | CanvasOpBody::Fill { opacity, .. } => opacity.clamp(0.0, 1.0),
        CanvasOpBody::Erase { .. } | CanvasOpBody::Clear | CanvasOpBody::Undo => 1.0,
    }
}

pub fn canvas_op_body_dash(body: &CanvasOpBody) -> &[f32] {
    match body {
        CanvasOpBody::Stroke { dash, .. }
        | CanvasOpBody::Line { dash, .. }
        | CanvasOpBody::Spline { dash, .. }
        | CanvasOpBody::Path { dash, .. }
        | CanvasOpBody::Rect { dash, .. }
        | CanvasOpBody::Ellipse { dash, .. } => dash.as_slice(),
        CanvasOpBody::Erase { .. }
        | CanvasOpBody::Fill { .. }
        | CanvasOpBody::Clear
        | CanvasOpBody::Undo => &[],
    }
}

pub fn canvas_op_body_gradient(body: &CanvasOpBody) -> Option<&CanvasLinearGradient> {
    match body {
        CanvasOpBody::Rect { gradient, .. }
        | CanvasOpBody::Ellipse { gradient, .. }
        | CanvasOpBody::Path { gradient, .. } => gradient.as_ref(),
        _ => None,
    }
}

pub fn set_canvas_op_body_opacity(body: &mut CanvasOpBody, opacity: f32) {
    let o = opacity.clamp(0.0, 1.0);
    match body {
        CanvasOpBody::Stroke { opacity, .. }
        | CanvasOpBody::Rect { opacity, .. }
        | CanvasOpBody::Ellipse { opacity, .. }
        | CanvasOpBody::Line { opacity, .. }
        | CanvasOpBody::Spline { opacity, .. }
        | CanvasOpBody::Path { opacity, .. }
        | CanvasOpBody::Fill { opacity, .. } => *opacity = o,
        CanvasOpBody::Erase { .. } | CanvasOpBody::Clear | CanvasOpBody::Undo => {}
    }
}

pub fn set_canvas_op_body_dash(body: &mut CanvasOpBody, pattern: Vec<f32>) {
    match body {
        CanvasOpBody::Stroke { dash, .. }
        | CanvasOpBody::Line { dash, .. }
        | CanvasOpBody::Spline { dash, .. }
        | CanvasOpBody::Path { dash, .. }
        | CanvasOpBody::Rect { dash, .. }
        | CanvasOpBody::Ellipse { dash, .. } => *dash = pattern,
        _ => {}
    }
}

pub fn set_canvas_op_body_gradient(
    body: &mut CanvasOpBody,
    gradient: Option<CanvasLinearGradient>,
) {
    match body {
        CanvasOpBody::Rect { gradient: slot, .. }
        | CanvasOpBody::Ellipse { gradient: slot, .. }
        | CanvasOpBody::Path { gradient: slot, .. } => *slot = gradient,
        _ => {}
    }
}

pub fn resolve_canvas_op_style_ex(body: &mut CanvasOpBody, pen: &CanvasPenStyle) {
    crate::resolve_canvas_op_style(body, pen);
    match body {
        CanvasOpBody::Stroke { dash, .. }
        | CanvasOpBody::Line { dash, .. }
        | CanvasOpBody::Spline { dash, .. }
        | CanvasOpBody::Path { dash, .. }
        | CanvasOpBody::Rect { dash, .. }
        | CanvasOpBody::Ellipse { dash, .. } => {
            if dash.is_empty() && !pen.dash.is_empty() {
                *dash = pen.dash.clone();
            }
        }
        CanvasOpBody::Fill { .. }
        | CanvasOpBody::Erase { .. }
        | CanvasOpBody::Clear
        | CanvasOpBody::Undo => {}
    }
}

/// Sample gradient at normalized board coords (0..1).
pub fn sample_linear_gradient(g: &CanvasLinearGradient, x: f32, y: f32) -> Option<[u8; 3]> {
    let c0 = parse_rgb(&g.color0)?;
    let c1 = parse_rgb(&g.color1)?;
    let rad = g.angle_deg.to_radians();
    let ux = rad.cos();
    let uy = rad.sin();
    let t = (x * ux + y * uy).clamp(0.0, 1.0);
    Some([
        (c0[0] as f32 * (1.0 - t) + c1[0] as f32 * t).round() as u8,
        (c0[1] as f32 * (1.0 - t) + c1[1] as f32 * t).round() as u8,
        (c0[2] as f32 * (1.0 - t) + c1[2] as f32 * t).round() as u8,
    ])
}

pub fn parse_rgb(s: &str) -> Option<[u8; 3]> {
    let hex = normalize_canvas_color(s)?;
    let t = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&t[0..2], 16).ok()?;
    let g = u8::from_str_radix(&t[2..4], 16).ok()?;
    let b = u8::from_str_radix(&t[4..6], 16).ok()?;
    Some([r, g, b])
}

/// Parse an exported JSON sidecar back into a live document + aspect.
pub fn parse_canvas_sidecar_json(raw: &str) -> Result<(CanvasDoc, CanvasAspect), String> {
    #[derive(Deserialize)]
    struct Sidecar {
        canvas_aspect: CanvasAspect,
        session_id: String,
        next_seq: u64,
        ops: Vec<CanvasOp>,
        pen: CanvasPenStyle,
        #[serde(default)]
        layers: Vec<CanvasLayer>,
        #[serde(default)]
        active_layer_id: String,
        #[serde(default)]
        next_layer_id: u64,
    }
    let sidecar: Sidecar = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    let doc = CanvasDoc {
        session_id: sidecar.session_id,
        next_seq: sidecar.next_seq,
        ops: sidecar.ops,
        pen: sidecar.pen,
        layers: sidecar.layers,
        active_layer_id: sidecar.active_layer_id,
        next_layer_id: sidecar.next_layer_id,
    };
    Ok((doc, sidecar.canvas_aspect))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CanvasOpBody, CanvasPenStyle, CanvasPoint};

    #[test]
    fn gradient_sample_interpolates() {
        let g = CanvasLinearGradient {
            color0: "#000000".into(),
            color1: "#ffffff".into(),
            angle_deg: 0.0,
        };
        assert_eq!(sample_linear_gradient(&g, 0.0, 0.5), Some([0, 0, 0]));
        assert_eq!(sample_linear_gradient(&g, 1.0, 0.5), Some([255, 255, 255]));
    }

    #[test]
    fn pen_dash_inherited_when_op_empty() {
        let pen = CanvasPenStyle {
            dash: vec![0.02, 0.02],
            ..CanvasPenStyle::default()
        };
        let mut stroke = CanvasOpBody::Stroke {
            points: vec![CanvasPoint { x: 0.0, y: 0.0 }],
            color: String::new(),
            width: 0.0,
            opacity: 1.0,
            dash: vec![],
        };
        resolve_canvas_op_style_ex(&mut stroke, &pen);
        if let CanvasOpBody::Stroke { dash, .. } = stroke {
            assert_eq!(dash, vec![0.02, 0.02]);
        } else {
            panic!("expected stroke");
        }
    }
}
