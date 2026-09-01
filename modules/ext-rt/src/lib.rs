// SPDX-License-Identifier: Apache-2.0
//! Module runtime scripté `ext-rt` — exécute `handlers.yaml` via host_call.
//!
//! Les modules agent « script » réutilisent ce binaire WASM ; seul
//! `handlers.yaml` + `manifest.yaml` changent par package.

fn handle(tool: &str, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    // Préférer handlers.json (produit par module.package) ; fallback handlers.yaml texte.
    let handlers_raw = aos_module_sdk::call(
        "ext.load_handlers",
        &serde_json::json!({"path": "handlers.json"}),
    )
    .or_else(|_| {
        aos_module_sdk::call(
            "ext.load_handlers",
            &serde_json::json!({"path": "handlers.yaml"}),
        )
    })?;
    let content = handlers_raw["content"]
        .as_str()
        .ok_or_else(|| "handlers illisible".to_string())?;
    let doc: serde_json::Value =
        parse_handlers(content).map_err(|e| format!("handlers: {e}"))?;
    let tools = doc
        .get("tools")
        .ok_or_else(|| "handlers: clé tools manquante".to_string())?;
    let steps_node = tools
        .get(tool)
        .ok_or_else(|| format!("outil non défini: {tool}"))?;
    let steps = steps_node
        .get("steps")
        .and_then(|s| s.as_array())
        .ok_or_else(|| "steps manquants".to_string())?;

    let mut last = serde_json::json!({});
    for step in steps {
        if let Some(ret) = step.get("return") {
            let rendered = render_value(ret, args, &last);
            return Ok(rendered);
        }
        let service = step
            .get("service")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "step.service manquant".to_string())?;
        let step_args = step.get("args").cloned().unwrap_or(serde_json::json!({}));
        let rendered = render_value(&step_args, args, &last);
        last = aos_module_sdk::call(service, &rendered)?;
    }
    Ok(last)
}

fn parse_handlers(content: &str) -> Result<serde_json::Value, String> {
    let trimmed = content.trim();
    serde_json::from_str(trimmed).map_err(|e| e.to_string())
}

fn render_value(
    v: &serde_json::Value,
    args: &serde_json::Value,
    last: &serde_json::Value,
) -> serde_json::Value {
    match v {
        serde_json::Value::String(s) => {
            serde_json::Value::String(render_str(s, args, last))
        }
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, val) in map {
                out.insert(k.clone(), render_value(val, args, last));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|x| render_value(x, args, last)).collect())
        }
        other => other.clone(),
    }
}

fn render_str(s: &str, args: &serde_json::Value, last: &serde_json::Value) -> String {
    let mut out = s.to_string();
    // {{args.foo}} / {{last.bar}}
    while let Some(start) = out.find("{{") {
        let Some(end) = out[start..].find("}}") else {
            break;
        };
        let end = start + end;
        let expr = &out[start + 2..end];
        let repl = resolve_expr(expr.trim(), args, last);
        out.replace_range(start..end + 2, &repl);
    }
    out
}

fn resolve_expr(expr: &str, args: &serde_json::Value, last: &serde_json::Value) -> String {
    let parts: Vec<&str> = expr.split('.').collect();
    if parts.is_empty() {
        return String::new();
    }
    let root = match parts[0] {
        "args" => args,
        "last" => last,
        _ => return expr.to_string(),
    };
    let mut cur = root;
    for p in &parts[1..] {
        cur = match cur.get(p) {
            Some(v) => v,
            None => return String::new(),
        };
    }
    match cur {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

aos_module_sdk::export_module!(handle);
