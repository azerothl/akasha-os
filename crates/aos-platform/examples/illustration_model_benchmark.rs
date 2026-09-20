//! Actual local model -> construction passes -> production raster. No recipes.
use aos_platform::illustration_raster;
use aos_proto::{IllustrationConstructionPhase as Phase, IllustrationDoc, IllustrationSpec};
use base64::Engine;
use serde_json::{json, Value};
use std::{fs, path::PathBuf, time::{Duration, SystemTime, UNIX_EPOCH}};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let model = args.get(1).map(String::as_str).unwrap_or("qwen3.5:9b");
    let subject = args.get(2).map(String::as_str).unwrap_or(
        "un vieux jardinier fumant la pipe dans un fauteuil regardant son jardin");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/illustration/model-results")
        .join(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis().to_string());
    fs::create_dir_all(&root)?;
    println!("Model: {model}\nArtifacts: {}", root.display());
    let system = r#"You are drawing an editorial illustration in five passes.
Return only a JSON IllustrationSpec, not code or prose. This is your actual drawing,
not a description. Preserve the user's subjects, viewpoint, pose, props and contacts.
Coordinates are 0..1 (y down). Design proportions, gesture, negative space, depth
and readable overlapping silhouettes before details. Do not stack disconnected circles.
Fields: construction_phase, skeleton, volumes, contours, details, parts, joint_bindings, contacts.
skeleton: [{id,parent:null or joint id,x,y,radius:0.008}]. Include anatomical joints and prop landmarks.
For a human, connect pelvis -> chest -> neck -> head; chest -> shoulders -> elbows -> wrists;
pelvis -> hips -> knees -> ankles. Parent references must exist and must not form cycles.
Choose coordinates for the requested gesture: seated thighs extend toward knees with shins
descending to supported feet, not two straight standing legs. Keep the whole figure framed.
volumes: [{id,kind:'rib_cage'/'pelvis'/'head_sphere'/'cylinder',x,y,w,h,rotation:0}].
Volume x/y is the TOP LEFT before rotation, never the center. Attach anatomical volumes
to skeleton joints using joint_bindings so they actually follow the pose.
parts/contours/details: [{id,role,fill_index:0,fill:true,outline:true,seed:1,
geometry:{kind:'path',points:[{x,y},...],closed:true}}]. Open paths may depict folds and features.
joint_bindings optionally maps part or volume id to [origin_joint,axis_joint]. Bound
coordinates become local: (0,0)=origin, (1,0)=axis, y perpendicular in bone lengths.
contacts optionally maps name to [root,bend,end,target] landmark ids, solving two-bone
reach without stretching. Independent chains only. Unreachable targets are errors.
Use matching bindings to keep masses, clothing and contours on the same pose.
Every response is a COMPLETE spec preserving earlier passes. Skeleton pass only needs
skeleton and construction. Volumes adds masses. Contours adds designed envelopes and
occlusion, suppressing internal construction boundaries. Details adds clothing/face/props.
Final assembles parts in back-to-front order including environment. No automatic artwork
will be supplied. Inspect the previous preview if attached, and correct visible mistakes.
Palette indices: 0 skin, 1 muted green, 2 taupe, 3 paper. Use expressive, economical lines."#;
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(600)).build()?;
    let mut previous = Value::Null;
    let mut preview: Option<Vec<u8>> = None;
    fs::write(root.join("run.json"), serde_json::to_vec_pretty(&json!({
        "model":model,"subject":subject,"system":system,"source":"ollama_local_no_recipe",
        "scope":"direct model and production raster; not full agent/UI integration"
    }))?)?;
    for (index, (name, phase)) in [
        ("skeleton",Phase::Skeleton),("volumes",Phase::Volumes),
        ("contours",Phase::Contours),("details",Phase::Details),("final",Phase::Final)
    ].into_iter().enumerate() {
        let mut feedback = String::new();
        for attempt in 0..3 {
        let prefix = format!("{index}-{name}-attempt{attempt}");
        println!("Generating {name}...");
        let mut user = json!({"role":"user","content":format!(
            "Request: {subject}\nProduce pass {name}. Previous complete spec: {previous}\nCorrections required before advancing: {feedback}")});
        if let Some(png) = &preview {
            user["images"] = json!([base64::engine::general_purpose::STANDARD.encode(png)]);
        }
        let request = json!({"model":model,"stream":false,"think":false,"format":"json",
            "options":{"temperature":0.25,"seed":42,"num_predict":6000,"num_ctx":16384},
            "messages":[{"role":"system","content":system},user]});
        let response: Value = client.post("http://127.0.0.1:11434/api/chat")
            .json(&request).send()?.error_for_status()?.json()?;
        fs::write(root.join(format!("{prefix}-response.json")),serde_json::to_vec_pretty(&response)?)?;
        let content = response["message"]["content"].as_str().ok_or("missing model content")?;
        let mut spec: IllustrationSpec = match serde_json::from_str(content) {
            Ok(spec) => spec,
            Err(error) => {
                feedback = format!("Invalid IllustrationSpec: {error}. Return valid JSON matching the provided schema.");
                if attempt == 2 { return Err(feedback.into()); }
                continue;
            }
        };
        if spec.construction_phase != phase {
            feedback = format!("Wrong construction_phase. Set it exactly to {name}, and produce that pass.");
            if attempt == 2 { return Err(feedback.into()); }
            continue;
        }
        spec.brief.subject = subject.into();
        spec.brief.look = aos_proto::IllustrationLook::Pencil;
        spec.brief.palette = spec.brief.look.default_palette();
        previous = serde_json::to_value(&spec)?;
        fs::write(root.join(format!("{prefix}-spec.json")),serde_json::to_vec_pretty(&previous)?)?;
        let doc = IllustrationDoc {brief:spec.brief.clone(),spec:Some(spec),..Default::default()};
        let png = match illustration_raster::export_png(&doc,768,768) {
            Ok(png) => png,
            Err(error) => {
                feedback = format!("Construction cannot render: {error}. Repair the geometry or constraints.");
                if attempt == 2 { return Err(feedback.into()); }
                continue;
            }
        };
        fs::write(root.join(format!("{prefix}.png")),&png)?;
        let criteria = match phase {
            Phase::Skeleton => "This is ONLY a stick-figure pose diagram. Lines represent bones and dots are joints. Judge ONLY gesture (seated vs standing), proportions, joint connections, limb placement and framing. Faces, clothing, skin, garden, smoke and finished furniture are intentionally absent; never request them. Suggest changes to joint coordinates/parents, not new drawing details.",
            Phase::Volumes => "This is ONLY construction masses. Judge placement, orientation, proportions and agreement with the skeleton. Clothing, faces, textures, colors and garden details are intentionally absent. Suggest mass position/size/binding corrections.",
            Phase::Contours => "This is a clean silhouette construction pass. Judge recognizable subject shapes, anatomy, contact and front/back occlusion. Fine facial details, textures and shading are not required yet.",
            Phase::Details | Phase::Final => "Judge the requested identity, action, anatomical proportions, contact, support, viewpoint, props and environment. Reject generic disconnected blobs or incorrect pose. Final must read as a coherent illustration.",
        };
        let critique: Value = client.post("http://127.0.0.1:11434/api/chat")
            .json(&json!({"model":model,"stream":false,"think":false,"format":"json",
                "options":{"temperature":0.1,"num_predict":1600,"num_ctx":16384},
                "messages":[{"role":"system","content":
                    format!("You are a drawing instructor evaluating ONE construction pass, not a finished picture. {criteria} Return JSON {{acceptable:boolean, problems:[specific visible defects], corrections:[concrete geometric changes]}}. Do not infer success from labels or merely present parts.")},
                    {"role":"user","content":format!("Request: {subject}\nCurrent pass: {name}\nPose data: {}", previous["skeleton"]),
                    "images":[base64::engine::general_purpose::STANDARD.encode(&png)]}]}))
            .send()?.error_for_status()?.json()?;
        fs::write(root.join(format!("{prefix}-critique-response.json")),serde_json::to_vec_pretty(&critique)?)?;
        let judgment: Value = serde_json::from_str(critique["message"]["content"].as_str().ok_or("missing critique")?)?;
        feedback = serde_json::to_string(&judgment)?;
        preview = Some(png);
        println!("Published {name}, attempt {attempt}: {feedback}");
        if judgment["acceptable"].as_bool() == Some(true) { break; }
        if attempt == 2 {
            return Err(format!("{name} rejected after three attempts; inspect artifacts, do not advance").into());
        }
        }
    }
    println!("All passes rendered. Visual quality remains to be reviewed.");
    Ok(())
}
