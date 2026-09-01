// SPDX-License-Identifier: Apache-2.0
//! Module « canvas » — dessin vectoriel partagé dans une session chat.
//!
//! Outils exposés aux agents via `module.invoke` ; le document vit côté
//! platformd (`host_call canvas.apply` / `canvas.get` / `canvas.export`).

use serde::Deserialize;
use serde_json::{json, Value};

fn handle(tool: &str, args: &Value) -> Result<Value, String> {
    match tool {
        "canvas.set_style" => set_style(args),
        "canvas.stroke" => stroke(args),
        "canvas.line" => line(args),
        "canvas.spline" => spline(args),
        "canvas.path" => path(args),
        "canvas.rect" => rect(args),
        "canvas.ellipse" => ellipse(args),
        "canvas.fill" => fill(args),
        "canvas.erase" => erase(args),
        "canvas.clear" => clear(args),
        "canvas.undo" => undo(args),
        "canvas.get" => get(args),
        "canvas.export" => export(args),
        _ => Err(format!("outil inconnu: {tool}")),
    }
}

aos_module_sdk::export_module!(handle);

fn require_session(args: &Value) -> Result<String, String> {
    args.get("session_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "session_id requis".into())
}

fn author_id(args: &Value) -> String {
    args.get("author_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("agent")
        .to_string()
}

fn apply(session_id: &str, author_id: &str, op: Value) -> Result<Value, String> {
    aos_module_sdk::call(
        "canvas.apply",
        &json!({
            "session_id": session_id,
            "author_id": author_id,
            "op": op,
        }),
    )
}

#[derive(Deserialize)]
struct StyleArgs {
    session_id: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    width: Option<f32>,
}

fn set_style(args: &Value) -> Result<Value, String> {
    let a: StyleArgs = aos_module_sdk::parse_args(args)?;
    let mut payload = json!({ "session_id": a.session_id });
    if let Some(c) = a.color {
        payload["color"] = json!(c);
    }
    if let Some(w) = a.width {
        payload["width"] = json!(w);
    }
    aos_module_sdk::call("canvas.set_style", &payload)
}

#[derive(Deserialize)]
struct StrokeArgs {
    session_id: String,
    #[serde(default)]
    author_id: Option<String>,
    points: Vec<Point>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    width: Option<f32>,
}

#[derive(Deserialize)]
struct Point {
    x: f32,
    y: f32,
}

fn stroke_op(points: Vec<Value>, color: Option<String>, width: Option<f32>) -> Value {
    let mut op = json!({ "kind": "stroke", "points": points });
    if let Some(c) = color {
        op["color"] = json!(c);
    }
    if let Some(w) = width {
        op["width"] = json!(w);
    }
    op
}

fn stroke(args: &Value) -> Result<Value, String> {
    let a: StrokeArgs = aos_module_sdk::parse_args(args)?;
    if a.points.len() < 2 {
        return Err("points: au moins 2 points requis".into());
    }
    let points: Vec<Value> = a
        .points
        .iter()
        .map(|p| json!({"x": p.x, "y": p.y}))
        .collect();
    apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        stroke_op(points, a.color, a.width),
    )
}

#[derive(Deserialize)]
struct LineArgs {
    session_id: String,
    #[serde(default)]
    author_id: Option<String>,
    p0: Point,
    p1: Point,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    width: Option<f32>,
}

fn line(args: &Value) -> Result<Value, String> {
    let a: LineArgs = aos_module_sdk::parse_args(args)?;
    let mut op = json!({
        "kind": "line",
        "p0": {"x": a.p0.x, "y": a.p0.y},
        "p1": {"x": a.p1.x, "y": a.p1.y},
    });
    if let Some(c) = a.color {
        op["color"] = json!(c);
    }
    if let Some(w) = a.width {
        op["width"] = json!(w);
    }
    apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        op,
    )
}

fn spline(args: &Value) -> Result<Value, String> {
    let a: StrokeArgs = aos_module_sdk::parse_args(args)?;
    if a.points.len() < 2 {
        return Err("points: au moins 2 points requis pour spline".into());
    }
    let points: Vec<Value> = a
        .points
        .iter()
        .map(|p| json!({"x": p.x, "y": p.y}))
        .collect();
    let mut op = json!({ "kind": "spline", "points": points });
    if let Some(c) = a.color {
        op["color"] = json!(c);
    }
    if let Some(w) = a.width {
        op["width"] = json!(w);
    }
    apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        op,
    )
}

#[derive(Deserialize)]
struct PathArgs {
    session_id: String,
    #[serde(default)]
    author_id: Option<String>,
    points: Vec<Point>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    width: Option<f32>,
    #[serde(default)]
    fill: Option<bool>,
    #[serde(default)]
    closed: Option<bool>,
}

fn path_bbox(points: &[Point]) -> (f32, f32, f32, f32) {
    let mut x0 = f32::INFINITY;
    let mut y0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    let mut y1 = f32::NEG_INFINITY;
    for p in points {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    (x0, y0, x1, y1)
}

fn path_success_message(points: &[Point], resp: &Value) -> String {
    let seq = resp
        .get("next_seq")
        .and_then(|v| v.as_u64())
        .or_else(|| resp.pointer("/applied/seq").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let (x0, y0, x1, y1) = path_bbox(points);
    format!(
        "ok seq={seq} path bbox=({x0:.3},{y0:.3})-({x1:.3},{y1:.3})"
    )
}

fn path(args: &Value) -> Result<Value, String> {
    let a: PathArgs = aos_module_sdk::parse_args(args)?;
    if a.points.len() < 3 {
        return Err("points: au moins 3 points requis pour path (silhouette)".into());
    }
    let points: Vec<Value> = a
        .points
        .iter()
        .map(|p| json!({"x": p.x, "y": p.y}))
        .collect();
    let mut op = json!({
        "kind": "path",
        "points": points,
        "fill": a.fill.unwrap_or(true),
        "closed": a.closed.unwrap_or(true),
    });
    if let Some(c) = a.color {
        op["color"] = json!(c);
    }
    if let Some(w) = a.width {
        op["width"] = json!(w);
    }
    let resp = apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        op,
    )?;
    Ok(json!(path_success_message(&a.points, &resp)))
}

#[derive(Deserialize)]
struct FillArgs {
    session_id: String,
    #[serde(default)]
    author_id: Option<String>,
    x: f32,
    y: f32,
    #[serde(default)]
    color: Option<String>,
}

fn fill(args: &Value) -> Result<Value, String> {
    let a: FillArgs = aos_module_sdk::parse_args(args)?;
    let mut op = json!({
        "kind": "fill",
        "x": a.x,
        "y": a.y,
    });
    if let Some(c) = a.color {
        op["color"] = json!(c);
    }
    apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        op,
    )
}

struct ShapeArgs {
    session_id: String,
    author_id: Option<String>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Option<String>,
    fill: bool,
    width: Option<f32>,
}

fn parse_f32_field(args: &Value, key: &str) -> Option<f32> {
    args.get(key)
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .filter(|v| v.is_finite())
}

fn shape_bbox_contract(tool: &str) -> String {
    format!(
        "{tool} attend x,y,w,h (coin haut-gauche + taille, coords 0..1, y vers le bas) — \
         pas le centre. Alias : cx,cy,w,h ; cx,cy,rx,ry ; x1,y1,x2,y2. \
         Ex: {{\"x\":0.35,\"y\":0.15,\"w\":0.30,\"h\":0.12,\"fill\":true}}"
    )
}

fn format_shape_bbox(x: f32, y: f32, w: f32, h: f32) -> String {
    format!(
        "({:.3},{:.3})-({:.3},{:.3})",
        x,
        y,
        x + w,
        y + h
    )
}

fn shape_success_message(kind: &str, a: &ShapeArgs, resp: &Value) -> String {
    let seq = resp
        .get("next_seq")
        .and_then(|v| v.as_u64())
        .or_else(|| resp.pointer("/applied/seq").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    format!(
        "ok seq={seq} {kind} bbox={}",
        format_shape_bbox(a.x, a.y, a.w, a.h)
    )
}

fn alias_shape_dimension_fields(args: &Value) -> Value {
    let mut out = args.clone();
    let Some(obj) = out.as_object_mut() else {
        return out;
    };
    if !obj.contains_key("w") && !obj.contains_key("h") {
        if obj.contains_key("width") && obj.contains_key("height") {
            if let Some(v) = obj.get("width").cloned() {
                obj.insert("w".into(), v);
            }
            if let Some(v) = obj.get("height").cloned() {
                obj.insert("h".into(), v);
            }
        }
    }
    out
}

/// Parse rect/ellipse args: canonical `x,y,w,h`, or aliases
/// `cx,cy,w,h` / `cx,cy,rx,ry` / `x,y,rx,ry` / `x1,y1,x2,y2`.
fn parse_shape_args(args: &Value, tool: &str) -> Result<ShapeArgs, String> {
    let args = alias_shape_dimension_fields(args);
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{tool}: session_id requis"))?;

    let author_id = args
        .get("author_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let (x, y, w, h) = if let (Some(x), Some(y), Some(w), Some(h)) = (
        parse_f32_field(&args, "x"),
        parse_f32_field(&args, "y"),
        parse_f32_field(&args, "w"),
        parse_f32_field(&args, "h"),
    ) {
        (x, y, w, h)
    } else if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
        parse_f32_field(&args, "x1"),
        parse_f32_field(&args, "y1"),
        parse_f32_field(&args, "x2"),
        parse_f32_field(&args, "y2"),
    ) {
        let x = x1.min(x2);
        let y = y1.min(y2);
        let w = (x2 - x1).abs();
        let h = (y2 - y1).abs();
        (x, y, w, h)
    } else if let (Some(cx), Some(cy), Some(w), Some(h)) = (
        parse_f32_field(&args, "cx"),
        parse_f32_field(&args, "cy"),
        parse_f32_field(&args, "w"),
        parse_f32_field(&args, "h"),
    ) {
        (cx - w / 2.0, cy - h / 2.0, w, h)
    } else {
        let cx = parse_f32_field(&args, "cx").or_else(|| parse_f32_field(&args, "x"));
        let cy = parse_f32_field(&args, "cy").or_else(|| parse_f32_field(&args, "y"));
        let rx = parse_f32_field(&args, "rx");
        let ry = parse_f32_field(&args, "ry");
        match (cx, cy, rx, ry) {
            (Some(cx), Some(cy), Some(rx), Some(ry)) => (cx - rx, cy - ry, 2.0 * rx, 2.0 * ry),
            _ => return Err(shape_bbox_contract(tool)),
        }
    };

    let color = args
        .get("color")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let fill = args.get("fill").and_then(|v| v.as_bool()).unwrap_or(false);
    let width = parse_f32_field(&args, "width");

    Ok(ShapeArgs {
        session_id,
        author_id,
        x,
        y,
        w,
        h,
        color,
        fill,
        width,
    })
}

fn shape_op(kind: &str, a: &ShapeArgs) -> Value {
    let mut op = json!({
        "kind": kind,
        "x": a.x, "y": a.y, "w": a.w, "h": a.h,
        "fill": a.fill,
    });
    if let Some(c) = &a.color {
        op["color"] = json!(c);
    }
    if let Some(w) = a.width {
        op["width"] = json!(w);
    }
    op
}

fn rect(args: &Value) -> Result<Value, String> {
    let a = parse_shape_args(args, "canvas.rect")?;
    let resp = apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        shape_op("rect", &a),
    )?;
    Ok(json!(shape_success_message("rect", &a, &resp)))
}

fn ellipse(args: &Value) -> Result<Value, String> {
    let a = parse_shape_args(args, "canvas.ellipse")?;
    let resp = apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        shape_op("ellipse", &a),
    )?;
    Ok(json!(shape_success_message("ellipse", &a, &resp)))
}

#[derive(Deserialize)]
struct EraseArgs {
    session_id: String,
    #[serde(default)]
    author_id: Option<String>,
    points: Vec<Point>,
    #[serde(default)]
    width: Option<f32>,
}

fn erase(args: &Value) -> Result<Value, String> {
    let a: EraseArgs = aos_module_sdk::parse_args(args)?;
    if a.points.is_empty() {
        return Err("points requis".into());
    }
    let points: Vec<Value> = a
        .points
        .iter()
        .map(|p| json!({"x": p.x, "y": p.y}))
        .collect();
    let mut op = json!({ "kind": "erase", "points": points });
    if let Some(w) = a.width {
        op["width"] = json!(w);
    }
    apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        op,
    )
}

fn clear(args: &Value) -> Result<Value, String> {
    let sid = require_session(args)?;
    apply(&sid, &author_id(args), json!({"kind": "clear"}))
}

fn undo(args: &Value) -> Result<Value, String> {
    let sid = require_session(args)?;
    apply(&sid, &author_id(args), json!({"kind": "undo"}))
}

fn get(args: &Value) -> Result<Value, String> {
    let sid = require_session(args)?;
    let mut payload = json!({"session_id": sid});
    if let Some(after) = args.get("after_seq") {
        payload["after_seq"] = after.clone();
    }
    aos_module_sdk::call("canvas.get", &payload)
}

fn export(args: &Value) -> Result<Value, String> {
    let sid = require_session(args)?;
    let mut payload = json!({"session_id": sid});
    if let Some(p) = args.get("path") {
        payload["path"] = p.clone();
    }
    if let Some(w) = args.get("width") {
        payload["width"] = w.clone();
    }
    if let Some(h) = args.get("height") {
        payload["height"] = h.clone();
    }
    aos_module_sdk::call("canvas.export", &payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shape_args_width_height_aliases_to_w_h() {
        let args = json!({
            "session_id": "s1",
            "x": 0.10,
            "y": 0.20,
            "width": 0.40,
            "height": 0.25,
            "fill": true
        });
        let a = parse_shape_args(&args, "canvas.rect").expect("width/height aliases");
        assert!((a.w - 0.40).abs() < 1e-6);
        assert!((a.h - 0.25).abs() < 1e-6);
    }

    #[test]
    fn shape_args_width_stays_stroke_when_w_h_present() {
        let args = json!({
            "session_id": "s1",
            "x": 0.10,
            "y": 0.20,
            "w": 0.40,
            "h": 0.25,
            "width": 0.01
        });
        let a = parse_shape_args(&args, "canvas.rect").expect("stroke width");
        assert!((a.w - 0.40).abs() < 1e-6);
        assert!((a.width.unwrap() - 0.01).abs() < 1e-6);
    }

    #[test]
    fn shape_args_canonical_bbox() {
        let args = json!({
            "session_id": "s1",
            "x": 0.35,
            "y": 0.15,
            "w": 0.30,
            "h": 0.12,
            "fill": true
        });
        let a = parse_shape_args(&args, "canvas.ellipse").expect("canonical");
        assert!((a.x - 0.35).abs() < 1e-6);
        assert!((a.y - 0.15).abs() < 1e-6);
        assert!((a.w - 0.30).abs() < 1e-6);
        assert!((a.h - 0.12).abs() < 1e-6);
        assert!(a.fill);
    }

    #[test]
    fn shape_args_svg_center_radii_cx_cy() {
        let args = json!({
            "session_id": "s1",
            "cx": 0.50,
            "cy": 0.40,
            "rx": 0.15,
            "ry": 0.06
        });
        let a = parse_shape_args(&args, "canvas.ellipse").expect("cx,cy,rx,ry");
        assert!((a.x - 0.35).abs() < 1e-6);
        assert!((a.y - 0.34).abs() < 1e-6);
        assert!((a.w - 0.30).abs() < 1e-6);
        assert!((a.h - 0.12).abs() < 1e-6);
    }

    #[test]
    fn shape_args_svg_center_radii_x_y() {
        let args = json!({
            "session_id": "s1",
            "x": 0.50,
            "y": 0.40,
            "rx": 0.15,
            "ry": 0.06
        });
        let a = parse_shape_args(&args, "canvas.ellipse").expect("x,y,rx,ry");
        assert!((a.x - 0.35).abs() < 1e-6);
        assert!((a.w - 0.30).abs() < 1e-6);
    }

    #[test]
    fn shape_args_two_corners() {
        let args = json!({
            "session_id": "s1",
            "x1": 0.60,
            "y1": 0.20,
            "x2": 0.30,
            "y2": 0.35
        });
        let a = parse_shape_args(&args, "canvas.rect").expect("x1,y1,x2,y2");
        assert!((a.x - 0.30).abs() < 1e-6);
        assert!((a.y - 0.20).abs() < 1e-6);
        assert!((a.w - 0.30).abs() < 1e-6);
        assert!((a.h - 0.15).abs() < 1e-6);
    }

    #[test]
    fn shape_args_prefers_canonical_over_radii() {
        let args = json!({
            "session_id": "s1",
            "x": 0.10,
            "y": 0.20,
            "w": 0.40,
            "h": 0.25,
            "rx": 0.99,
            "ry": 0.99
        });
        let a = parse_shape_args(&args, "canvas.ellipse").expect("canonical wins");
        assert!((a.w - 0.40).abs() < 1e-6);
        assert!((a.h - 0.25).abs() < 1e-6);
    }

    #[test]
    fn shape_args_center_size_cx_cy_w_h() {
        let args = json!({
            "session_id": "s1",
            "cx": 0.50,
            "cy": 0.40,
            "w": 0.30,
            "h": 0.12
        });
        let a = parse_shape_args(&args, "canvas.ellipse").expect("cx,cy,w,h");
        assert!((a.x - 0.35).abs() < 1e-6);
        assert!((a.y - 0.34).abs() < 1e-6);
        assert!((a.w - 0.30).abs() < 1e-6);
        assert!((a.h - 0.12).abs() < 1e-6);
    }

    #[test]
    fn shape_success_message_formats_bbox_echo() {
        let a = ShapeArgs {
            session_id: "s1".into(),
            author_id: None,
            x: 0.35,
            y: 0.15,
            w: 0.30,
            h: 0.12,
            color: None,
            fill: true,
            width: None,
        };
        let resp = json!({"next_seq": 12});
        let msg = shape_success_message("ellipse", &a, &resp);
        assert_eq!(msg, "ok seq=12 ellipse bbox=(0.350,0.150)-(0.650,0.270)");
    }

    #[test]
    fn shape_args_error_names_contract_not_single_field() {
        let args = json!({"session_id": "s1", "x1": 0.1, "y1": 0.2});
        match parse_shape_args(&args, "canvas.ellipse") {
            Err(err) => {
                assert!(err.contains("canvas.ellipse attend x,y,w,h"));
                assert!(err.contains("pas le centre"));
                assert!(!err.contains("missing field"));
            }
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn path_success_message_formats_bbox() {
        let points = vec![
            Point { x: 0.1, y: 0.7 },
            Point { x: 0.5, y: 0.55 },
            Point { x: 0.9, y: 0.7 },
        ];
        let resp = json!({"next_seq": 7});
        let msg = path_success_message(&points, &resp);
        assert_eq!(msg, "ok seq=7 path bbox=(0.100,0.550)-(0.900,0.700)");
    }

    #[test]
    fn shape_op_emits_bbox_fields() {
        let a = ShapeArgs {
            session_id: "s1".into(),
            author_id: None,
            x: 0.35,
            y: 0.15,
            w: 0.30,
            h: 0.12,
            color: Some("#c0c0c0".into()),
            fill: true,
            width: None,
        };
        let op = shape_op("ellipse", &a);
        assert_eq!(op["kind"], "ellipse");
        assert!((op["x"].as_f64().unwrap() - 0.35).abs() < 1e-6);
        assert!((op["w"].as_f64().unwrap() - 0.30).abs() < 1e-6);
        assert_eq!(op["fill"], true);
        assert_eq!(op["color"], "#c0c0c0");
    }
}
