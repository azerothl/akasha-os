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

#[derive(Deserialize)]
struct ShapeArgs {
    session_id: String,
    #[serde(default)]
    author_id: Option<String>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    fill: bool,
    #[serde(default)]
    width: Option<f32>,
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
    let a: ShapeArgs = aos_module_sdk::parse_args(args)?;
    apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        shape_op("rect", &a),
    )
}

fn ellipse(args: &Value) -> Result<Value, String> {
    let a: ShapeArgs = aos_module_sdk::parse_args(args)?;
    apply(
        &a.session_id,
        a.author_id.as_deref().unwrap_or("agent"),
        shape_op("ellipse", &a),
    )
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
