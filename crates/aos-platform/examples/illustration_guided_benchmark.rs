//! Actual local planner -> image editing. No hand-authored construction fixture.
//! Args: MODEL_DIR NEW_OUTPUT_DIR SUBJECT [PLANNER_MODEL] [IMAGE_SEED]
use aos_sd::{illustration::generate_planned_passes, ImageGenOpts};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{fs, path::PathBuf, time::Duration};

#[derive(Deserialize)]
struct Plan {
    construction: String,
    frame_subject: String,
    scale_relationships: Vec<String>,
    contacts: Vec<String>,
    subjects: Vec<String>,
    spatial_relations: Vec<String>,
    requested_elements: Vec<RequestedElement>,
}

#[derive(Deserialize)]
struct RequestedElement {
    source_excerpt: String,
    visual_evidence: String,
    in_selected_frame: bool,
}

fn frame_evidence(elements: &[RequestedElement]) -> String {
    elements.iter().filter(|element| element.in_selected_frame)
        .map(|element| element.visual_evidence.as_str()).collect::<Vec<_>>().join("\n")
}

// This only verifies quotation provenance, not completeness or visual truth.
fn grounded_elements(subject: &str, elements: &[RequestedElement]) -> bool {
    let original = subject.to_lowercase();
    !elements.is_empty() && elements.iter().all(|element|
        !element.source_excerpt.trim().is_empty() && !element.visual_evidence.trim().is_empty()
        && original.contains(&element.source_excerpt.trim().to_lowercase()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 { return Err("expected MODEL_DIR NEW_OUTPUT_DIR SUBJECT [PLANNER_MODEL] [IMAGE_SEED]".into()); }
    let model_dir = PathBuf::from(&args[1]);
    let root = PathBuf::from(&args[2]);
    let subject = &args[3];
    let planner = args.get(4).map(String::as_str).unwrap_or("qwen3.5:9b");
    let image_seed: u32 = args.get(5).map(|seed| seed.parse()).transpose()?.unwrap_or(42);
    let weights = model_dir.join("flux-2-klein-4b-Q8_0.gguf");
    let opts = ImageGenOpts {
        width: 768, height: 768, steps: 4, cfg_scale: Some(1.0), seed: Some(i64::from(image_seed)),
        sampling_method: Some("euler".into()), diffusion_model: Some(weights.clone()),
        llm_path: Some(model_dir.join("Qwen3-4B-Q4_K_M.gguf")),
        vae_path: Some(model_dir.join("split_files/vae/flux2-vae.safetensors")),
        offload_to_cpu: true, diffusion_fa: true, ..Default::default()
    };
    for path in [&weights, opts.llm_path.as_ref().unwrap(), opts.vae_path.as_ref().unwrap()] {
        if !path.is_file() { return Err(format!("missing model: {}", path.display()).into()); }
    }
    fs::create_dir(&root)?;
    fs::write(root.join("image-seed.txt"), image_seed.to_string())?;
    let system = r#"You are planning the first gesture/construction pass of a drawing.
Return JSON {"requested_elements":[{"source_excerpt":string,"visual_evidence":string,"in_selected_frame":boolean}],"construction":string,"frame_subject":string,"scale_relationships":[string],"contacts":[string],"subjects":[string],"spatial_relations":[string]}.
Choose ONE physically possible instant before writing the inventory. Do not
combine successive actions into a hybrid pose. In requested_elements, set
in_selected_frame=true for subjects, objects, attributes and actions actually
visible in that instant. Set it to false for earlier or later actions; keep them
in the inventory for future frames, but exclude them from construction,
frame_subject, scale_relationships and contacts. A future relaxed position must
not distort the anatomy of a current airborne pose, or vice versa.
In requested_elements, cover every requested subject, object, attribute
and action. source_excerpt is an exact short quotation from the original user
request, in its original language. visual_evidence is an English description of
the visible features that make that element recognizable in the selected frame,
including distinctive functional parts of objects and visible action evidence.
Do not add inventory entries without a supporting quotation. Build construction
and frame_subject from this inventory; do not add unrelated objects to fill space.
scale_relationships must contain explicit plausible numeric size ratios between
the named subjects/furniture (or main body masses for a lone subject). Specify
which length/height is compared, so a ratio cannot mean the tail or a different axis.
contacts must name the exact surface and body part that touches it or approaches
it in this instant. Distinguish a seat from a backrest, a hand from an elbow, and
an airborne near-contact from a planted support. These arrays must not be empty.
frame_subject describes exactly ONE instant to illustrate, including the requested
identities, details and style but only the action visible at that instant. If the
request contains successive actions, select one keyframe: omit the other actions
from frame_subject, not merely a warning to ignore them. Keep the subject count
explicit. The construction must describe that same instant.
Write frame_subject in clear English for the image model, preserving the user's
meaning regardless of their language. Describe the distinctive visible parts of
requested objects and the visible evidence of requested actions, not only their
names. Include functional parts in construction when they determine support or
contact. Use positive visual descriptions, not a list of forbidden additions.
Do not embellish the request with new characters, animals or narrative props.
Interpret the user's original request faithfully. Do not invent extra subjects.
construction is a concise English geometric description for an image model, not
coordinates or code. Specify each subject's topology (human, quadruped, bird,
object), posture, viewpoint, facing direction, relative scale, support and contact
with props. Describe near/far limbs clearly and keep their count correct. Establish
explicit plausible size ratios between subjects and furniture, using anatomical
landmarks rather than vague 'relative scale'. For movement onto a surface, specify
the actual destination surface, near/far occlusion, descending/ascending phase,
distance of contact points from that surface and the center-of-mass direction.
Choose a readable instant close to the requested interaction rather than an
unrelated airborne pose far above it. Carry those scale/contact relationships
into frame_subject as well. Do not invent exact anatomy from joint symbols:
articulation must follow the named species and its natural limb proportions.
Use a balanced whole-scene layout with space in the direction of the gaze or movement.
Use single action lines, joint locations, blank head ovals, simple object blocks
and background placement lines. Defer facial features, age marks, clothes, fur,
foliage, color, shading and finishing style to later passes. Do not ask for a
medical skeleton. Do not turn animals into human mannequins. Keep the construction
description under 180 words. subjects and spatial_relations list requirements to
check in the finished picture; they may include identity and details omitted from
the initial sketch. If a request contains several successive actions, describe
one explicit keyframe and state which action it depicts; do not claim a still
shows the entire sequence."#;
    let request = json!({"model":planner,"stream":false,"think":false,"format":"json",
        "keep_alive":0,"options":{"temperature":0.2,"seed":42,"num_predict":1800,"num_ctx":8192},
        "messages":[{"role":"system","content":system},{"role":"user","content":subject}]});
    fs::write(root.join("planner-request.json"), serde_json::to_vec_pretty(&request)?)?;
    println!("Planning with {planner}: {subject}");
    let response: Value = reqwest::blocking::Client::builder().timeout(Duration::from_secs(300)).build()?
        .post("http://127.0.0.1:11434/api/chat").json(&request).send()?.error_for_status()?.json()?;
    fs::write(root.join("planner-response.json"), serde_json::to_vec_pretty(&response)?)?;
    let content = response["message"]["content"].as_str().ok_or("missing plan content")?;
    let plan: Plan = serde_json::from_str(content)?;
    if plan.construction.trim().is_empty() || plan.frame_subject.trim().is_empty() || plan.subjects.is_empty() || plan.spatial_relations.is_empty()
        || plan.scale_relationships.is_empty() || plan.contacts.is_empty() {
        return Err("incomplete construction plan".into());
    }
    if !grounded_elements(subject, &plan.requested_elements) {
        return Err("ungrounded or empty requested element; inspect saved planner response".into());
    }
    if !plan.requested_elements.iter().any(|element| element.in_selected_frame) {
        return Err("no requested elements assigned to selected frame".into());
    }
    fs::write(root.join("plan.json"), content)?;
    println!("Construction: {}", plan.construction);
    aos_sd::clear_media_cancel();
    let constraints = format!("Relative sizes: {}\nContacts at this instant: {}", plan.scale_relationships.join("; "), plan.contacts.join("; "));
    let construction = format!("{}\n{}", plan.construction, constraints);
    let evidence = frame_evidence(&plan.requested_elements);
    let frame_subject = format!("{}\nRequested visible elements:\n{}\n{}", plan.frame_subject, evidence, constraints);
    generate_planned_passes(&weights, &frame_subject, &construction, &root.join("passes"), &opts,
        |phase, path| { println!("PASS READY {}: {}", phase.name(), path.display()); Ok(()) })?;
    println!("Rendered all passes. Visual fidelity and desktop integration remain unverified.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inventory_requires_actual_nonempty_user_quotes_and_visual_descriptions() {
        let element = |quote: &str, evidence: &str| RequestedElement {
            source_excerpt: quote.into(), visual_evidence: evidence.into(), in_selected_frame: true,
        };
        let subject = "un vieux jardinier fumant la pipe";
        assert!(grounded_elements(subject, &[element("VIEUX JARDINIER", "grey hair and lined face")]));
        assert!(!grounded_elements(subject, &[element("tabouret", "wooden stool")]));
        assert!(!grounded_elements(subject, &[element("", "wooden stool")]));
        assert!(!grounded_elements(subject, &[element("pipe", " ")]));
        assert!(!grounded_elements(subject, &[]));
    }

    #[test]
    fn later_actions_are_preserved_in_inventory_but_not_rendered_in_this_frame() {
        let elements = vec![
            RequestedElement { source_excerpt: "saute".into(), visual_evidence: "front paws reaching the cushion".into(), in_selected_frame: true },
            RequestedElement { source_excerpt: "se couche".into(), visual_evidence: "curled sleeping body".into(), in_selected_frame: false },
        ];
        assert_eq!(frame_evidence(&elements), "front paws reaching the cushion");
        assert_eq!(elements.len(), 2);
        assert!(!elements[1].in_selected_frame);
    }
}
