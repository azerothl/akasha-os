use aos_platform::illustration_raster;
use aos_proto::{
    enrich_illustration_puppet, review_illustration, IllustrationBrief, IllustrationDoc,
    IllustrationConstructionPhase, IllustrationLook, IllustrationSpec,
};
use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    prompt: String,
    look: String,
}

fn parse_look(value: &str) -> IllustrationLook {
    IllustrationLook::parse(value).unwrap_or(IllustrationLook::Ink)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or("workspace root not found")?
        .to_path_buf();
    let cases_path = root.join("benchmarks/illustration/cases.json");
    let output_root = root.join("benchmarks/illustration/results");
    fs::create_dir_all(&output_root)?;
    let cases: Vec<Case> = serde_json::from_slice(&fs::read(&cases_path)?)?;

    println!("Illustration benchmark: {} cases", cases.len());
    for case in cases {
        let look = parse_look(&case.look);
        let brief = IllustrationBrief {
            subject: case.prompt.clone(),
            look,
            palette: look.default_palette(),
            anchor: case.prompt.clone(),
            ..Default::default()
        };
        let mut spec = IllustrationSpec {
            brief: brief.clone(),
            ..Default::default()
        };
        enrich_illustration_puppet(&mut spec);
        let doc = IllustrationDoc {
            brief,
            spec: Some(spec),
            last_sheet_png: Some(format!("/benchmark/{}/sheet-240.png", case.id)),
            last_model_sheet_png: Some(format!("/benchmark/{}/model-sheet-240.png", case.id)),
            ..Default::default()
        };
        let dir = output_root.join(&case.id);
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join("still-1024.png"),
            illustration_raster::export_png(&doc, 1024, 1024)?,
        )?;
        let mut skeleton_doc = doc.clone();
        if let Some(ref mut phase_spec) = skeleton_doc.spec {
            phase_spec.construction_phase = IllustrationConstructionPhase::Skeleton;
        }
        fs::write(
            dir.join("skeleton-240.png"),
            illustration_raster::export_png(&skeleton_doc, 240, 240)?,
        )?;
        let mut volumes_doc = doc.clone();
        if let Some(ref mut phase_spec) = volumes_doc.spec {
            phase_spec.construction_phase = IllustrationConstructionPhase::Volumes;
        }
        fs::write(
            dir.join("volumes-240.png"),
            illustration_raster::export_png(&volumes_doc, 240, 240)?,
        )?;
        fs::write(
            dir.join("sheet-240.png"),
            illustration_raster::export_sheet_png(&doc, 240)?,
        )?;
        fs::write(
            dir.join("model-sheet-240.png"),
            illustration_raster::export_model_sheet_png(&doc, 240)?,
        )?;
        let review = review_illustration(&doc);
        fs::write(
            dir.join("review.json"),
            serde_json::to_vec_pretty(&review)?,
        )?;
        println!(
            "{}: score={:.2}, errors={}, warnings={}",
            case.id,
            review.score,
            review
                .issues
                .iter()
                .filter(|issue| issue.severity == "error")
                .count(),
            review
                .issues
                .iter()
                .filter(|issue| issue.severity == "warning")
                .count()
        );
    }
    println!("Outputs: {}", output_root.display());
    Ok(())
}
