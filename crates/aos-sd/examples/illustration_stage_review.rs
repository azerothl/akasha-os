//! Calibrate a local visual construction gate against saved actual images.
//! Args: PNG FRAME_SUBJECT PHASE NEW_OUTPUT_DIR [LOCAL_MODEL]. Does not approve final artwork.
use base64::Engine;
use serde_json::{json, Value};
use std::{fs, path::PathBuf, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if !(5..=6).contains(&args.len()) { return Err("expected PNG FRAME_SUBJECT PHASE NEW_OUTPUT_DIR [LOCAL_MODEL]".into()); }
    let model = args.get(5).map(String::as_str).unwrap_or("qwen3.5:9b");
    if model.contains(":cloud") { return Err("this benchmark permits local models only".into()); }
    if !["skeleton", "volumes", "contours", "details", "final"].contains(&args[3].as_str()) {
        return Err("unknown drawing phase".into());
    }
    let png = fs::read(&args[1])?;
    image::load_from_memory(&png)?;
    let root = PathBuf::from(&args[4]);
    fs::create_dir(&root)?;
    fs::write(root.join("input.png"), &png)?;
    let system = r#"Inspect the attached actual drawing, not just the requested description.
Return JSON {"decision":"accept"|"reject"|"uncertain", "observations":[{"subject":string,"part":string,"visible_count":number,"evidence":string}], "defects":[{"region":string,"visible_problem":string}], "correction":string}.
This is a gate before continuing a drawing, not final artistic approval.
First trace the visible body silhouettes and attachments. Count heads, main bodies,
limbs and tails/wings where relevant. Distinguish near/far limbs and occlusion.
For every animal, explicitly include observations for head, torso, forelimbs,
hindlimbs and tails (wings/fins as applicable). Do not skip a category because
the expected species count seems obvious. For each visible appendage, trace its
root along its entire contour to the endpoint and describe where it lies in the
image; a curved structure above a back is not automatically part of the back.
Do not infer the expected count from species alone: report what the pixels show.
Evidence must locate visible endpoints/attachments. Check species anatomy,
subject count, relative size, requested pose and actual support/contact surfaces.
Construction joint circles, axes, blank faces and simple masses are EXPECTED in
skeleton/volumes; they are not defects at those stages. Surface polish, clothing,
fur and shading are not required until details/final. A duplicated appendage,
disconnected limb or wrong contact is a defect even in an early sketch.
Reject concrete anatomical or scene errors; list localized evidence and one
targeted correction preserving the stage, valid pose, framing and subjects.
If visibility does not support a conclusion, return uncertain rather than invent
details. Accept only if the actual image has no supported blocking error. An
accept decision must have no defects and an empty correction. No confidence score."#;
    let request = json!({"model":model, "stream":false,"think":false,"format":"json","keep_alive":0,
        "options":{"temperature":0.0,"seed":42,"num_predict":2400,"num_ctx":12288},
        "messages":[{"role":"system","content":system},
            {"role":"user","content":format!("Frame to depict: {}\nCurrent drawing phase: {}",args[2],args[3]),
            "images":[base64::engine::general_purpose::STANDARD.encode(&png)]}]});
    // Avoid duplicating a large base64 image in the saved request evidence.
    let mut request_log = request.clone();
    request_log["messages"][1]["images"] = json!(["input.png"]);
    fs::write(root.join("request.json"), serde_json::to_vec_pretty(&request_log)?)?;
    let response: Value = reqwest::blocking::Client::builder().timeout(Duration::from_secs(240)).build()?
        .post("http://127.0.0.1:11434/api/chat").json(&request).send()?.error_for_status()?.json()?;
    fs::write(root.join("response.json"), serde_json::to_vec_pretty(&response)?)?;
    let review: Value = serde_json::from_str(response["message"]["content"].as_str().ok_or("missing review")?)?;
    fs::write(root.join("review.json"), serde_json::to_vec_pretty(&review)?)?;
    println!("{}", serde_json::to_string_pretty(&review)?);
    let observations = review["observations"].as_array().ok_or("missing observations")?;
    let defects = review["defects"].as_array().ok_or("missing defects")?;
    if observations.is_empty() { return Err("review lacks visual observations".into()); }
    match review["decision"].as_str() {
        Some("accept") if defects.is_empty() && review["correction"].as_str() == Some("") => Ok(()),
        Some("reject") if !defects.is_empty() => Err("visual construction gate rejected the drawing".into()),
        Some("uncertain") => Err("visual construction gate is uncertain; do not advance automatically".into()),
        _ => Err("invalid or contradictory stage review; do not advance".into()),
    }
}
