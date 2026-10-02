#!/usr/bin/env python3
from pathlib import Path

p = Path("crates/aos-platform/src/subsystem.rs")
text = p.read_text()
anchor = "            // Escalade interdite depuis WASM\n"
insert = '            "git.status" => {\n                let root = args["root"].as_str().unwrap_or("").to_string();\n                let req = aos_proto::workspace::GitStatusRequest {\n                    root,\n                    actor: ctx.actor.clone(),\n                    caps: ctx.granted_caps.clone(),\n                    trace_id: ctx.trace_id.clone(),\n                };\n                let resp = {\n                    let mgr = self.workspaces.lock().unwrap();\n                    crate::workspace_git::git_status(&mgr, &req)\n                };\n                if resp.ok {\n                    self.audit(AuditAppendRequest {\n                        trace_id: ctx.trace_id.clone(),\n                        actor: format!("module:{}", ctx.module),\n                        action: "git.status".into(),\n                        target: req.root.clone(),\n                        detail: serde_json::json!({\n                            "entries": resp.entries.len(),\n                            "truncated": resp.truncated,\n                            "on_behalf_of": ctx.actor,\n                        }),\n                    });\n                }\n                serde_json::to_value(resp).map_err(|e| e.to_string())\n            }\n            "git.diff" => {\n                let root = args["root"].as_str().unwrap_or("").to_string();\n                let path = args["path"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string());\n                let staged = args["staged"].as_bool().unwrap_or(false);\n                let req = aos_proto::workspace::GitDiffRequest {\n                    root,\n                    path,\n                    staged,\n                    actor: ctx.actor.clone(),\n                    caps: ctx.granted_caps.clone(),\n                    trace_id: ctx.trace_id.clone(),\n                };\n                let resp = {\n                    let mgr = self.workspaces.lock().unwrap();\n                    crate::workspace_git::git_diff(&mgr, &req)\n                };\n                if resp.ok {\n                    self.audit(AuditAppendRequest {\n                        trace_id: ctx.trace_id.clone(),\n                        actor: format!("module:{}", ctx.module),\n                        action: "git.diff".into(),\n                        target: req.root.clone(),\n                        detail: serde_json::json!({\n                            "staged": req.staged,\n                            "bytes": resp.diff.len(),\n                            "truncated": resp.truncated,\n                            "on_behalf_of": ctx.actor,\n                        }),\n                    });\n                }\n                serde_json::to_value(resp).map_err(|e| e.to_string())\n            }\n'
if '"git.status"' in text:
    print("subsystem already has git.status")
elif anchor not in text:
    raise SystemExit("anchor missing in subsystem.rs")
else:
    text = text.replace(anchor, insert + anchor, 1)
    p.write_text(text)
    print("subsystem patched", p.stat().st_size)

p = Path("docs/FEATURES.md")
text = p.read_text()
old1 = '- **Dev-assistant P0**: `workspace.bind` / `list` / `unbind` with caps `/host/<id>/**`; bounded `fs.search` / `code.search`; `fs.apply_patch` + multi-file undo; module contract + community reference package — [dev-assistant-p0.md](dev-assistant-p0.md)\n'
new_bullet = "- **Dev-assistant DA.5 (0.20 start)**: read-only `git.status` / `git.diff` on bound `/host/<id>` trees (requires `fs.read`; no commit/push/fetch)\n"
if "Dev-assistant DA.5" not in text:
    if old1 not in text:
        raise SystemExit("FEATURES EN old1 missing")
    text = text.replace(old1, old1 + new_bullet, 1)
old2 = "- Dev-assistant **P1–P2** (cap-gated `process.run`, git.*, LSP/DAP, DeclUI IDE widgets) — P0 bind/search/patch ships; deeper IDE track is **0.20+**\n"
new2 = "- Dev-assistant **remaining P1–P2** (cap-gated `process.run`, `git.commit`, LSP/DAP, DeclUI IDE widgets) — P0 bind/search/patch ships; read-only `git.status`/`git.diff` (DA.5) is the first **0.20** slice; deeper IDE track stays **0.20+**\n"
if old2 in text:
    text = text.replace(old2, new2, 1)
elif "remaining P1–P2" in text:
    print("FEATURES EN old2 already updated")
else:
    raise SystemExit("FEATURES EN old2 missing")
p.write_text(text)
print("FEATURES EN patched")

p = Path("docs/fr/FEATURES.md")
text = p.read_text()
old = "- Dev-assistant **P1–P2** (`process.run` cap-gated, git.*, LSP/DAP, widgets DeclUI IDE) — P0 bind/search/patch livré ; piste IDE profonde en **0.20+**\n"
new = "- Dev-assistant **P1–P2 restants** (`process.run`, `git.commit`, LSP/DAP, widgets IDE) — P0 livré ; DA.5 `git.status`/`git.diff` lecture seule = premier slice **0.20**\n"
if old in text:
    text = text.replace(old, new, 1)
elif "P1–P2 restants" in text:
    print("FEATURES FR already updated")
else:
    raise SystemExit("FEATURES FR old missing")
p.write_text(text)
print("FEATURES FR patched")
