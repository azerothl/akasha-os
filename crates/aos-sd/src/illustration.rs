//! Sequential image-editing passes. No reconstruction of fake intermediate
//! stages from a finished image, and no placeholder fallback.
use crate::{generate_image_strict_progress, ImageGenOpts, MediaError};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawingPass { Skeleton, Volumes, Contours, Details, Final }

impl DrawingPass {
    pub fn name(self) -> &'static str {
        match self {
            Self::Skeleton => "skeleton", Self::Volumes => "volumes",
            Self::Contours => "contours", Self::Details => "details", Self::Final => "final",
        }
    }

    fn instruction(self) -> &'static str {
        match self {
            Self::Skeleton => "An artist's very early gesture sketch in light pencil on white paper. Capture each described subject's natural pose with a flowing line of action and a few economical strokes indicating its overall shape, balance and direction. The head is a simple undecorated mass. Each appendage is one continuous gesture from its natural attachment to its endpoint, with the correct bends and foreshortening. Establish relative scale, support and contact with the scene using sparse perspective placement edges. This is a loose preparatory drawing with large white areas, not a finished outline drawing. Only pose and composition are visible at this stage.",
            Self::Volumes => "Develop the reference gesture into an artist's solid mass study of the SAME natural subjects. Preserve species, pose, framing, relative scale and contacts. Give the main masses distinct orientation, weight and depth with tapered curved forms and broad light planar shading. Articulation is expressed by continuous overlapping organic masses and changes of direction. Describe near and far surfaces by overlaps and foreshortening. Keep the head undecorated and surfaces plain. Objects become coherent perspective solids. This is an unfinished pencil block-in, with natural anatomical connections and readable three-dimensional form.",
            Self::Contours => "Redraw this construction study as clean natural outlines. Erase all circular joint markers, interior connecting rods, cross-section ellipses and perspective guide lines. Replace them with the continuous silhouette of each requested subject and the visible edges of real objects. Preserve pose, framing, species and contact, not construction marks. Unshaded line art. Animals have natural bodies, not clothing or mechanical joints.",
            Self::Details => "Develop the natural subjects with their requested identifying details, species-appropriate features, accessories and environment. For humans, clothing folds follow tension and gravity; for animals, natural fur, feathers or scales follow the body instead. Never dress an animal unless explicitly requested. Remove remaining construction symbols and guide lines. Preserve the pose, composition and contacts while clarifying the requested identity.",
            Self::Final => "Finish this same illustration with controlled line weight, restrained shading and harmonious color where appropriate to the requested style. Preserve all subjects, anatomy, contacts and composition. Finish every object as well as the figure: replace furniture construction boxes with recognizable functional furniture, coherent supports, thickness and material. Erase ALL remaining guide lines, protruding perspective edges and joint symbols. Only real visible edges belong in the finished scene. Do not redesign the composition or introduce extra limbs or props.",
        }
    }
}

/// Called after each decoded output, before beginning the next pass. The caller
/// can persist/publish the preview or reject it following visual review.
/// `output_dir` must not exist: a run never overwrites earlier evidence.
pub fn generate_passes<F>(
    weights: &Path, subject: &str, output_dir: &Path, options: &ImageGenOpts,
    on_pass: F,
) -> Result<Vec<PathBuf>, MediaError>
where F: FnMut(DrawingPass, &Path) -> Result<(), MediaError> {
    generate_planned_passes(weights, subject, subject, output_dir, options, on_pass)
}

/// `construction` describes only gesture, proportions, camera, support and
/// spatial relations. Keep identity, garments and finishing style in `subject`.
/// This separation lets a scene planner defer details instead of asking an image
/// model to ignore contradictory finished-art instructions in its first passes.
pub fn generate_planned_passes<F>(
    weights: &Path, subject: &str, construction: &str, output_dir: &Path,
    options: &ImageGenOpts, on_pass: F,
) -> Result<Vec<PathBuf>, MediaError>
where F: FnMut(DrawingPass, &Path) -> Result<(), MediaError> {
    run_passes(subject, construction, output_dir, options, |prompt, dest, opts| {
        generate_image_strict_progress(weights, prompt, dest, opts, |_, _| {})?;
        Ok(())
    }, on_pass)
}

/// Bootstrap from a pose guide without feeding that unfinished guide back into
/// later finishing passes. Later passes still consume their actual predecessor.
pub fn generate_pose_guided_passes<F>(
    weights: &Path, subject: &str, construction: &str, pose: &Path,
    output_dir: &Path, options: &ImageGenOpts, on_pass: F,
) -> Result<Vec<PathBuf>, MediaError>
where F: FnMut(DrawingPass, &Path) -> Result<(), MediaError> {
    let pose = std::fs::canonicalize(pose)?;
    run_pose_passes(subject, construction, &pose, output_dir, options,
        |prompt, dest, opts| {
            generate_image_strict_progress(weights, prompt, dest, opts, |_, _| {})?;
            Ok(())
        }, on_pass)
}

fn run_pose_passes<G, F>(subject: &str, construction: &str, pose: &Path,
    output_dir: &Path, options: &ImageGenOpts, mut generate: G, on_pass: F)
    -> Result<Vec<PathBuf>, MediaError>
where G: FnMut(&str, &Path, &ImageGenOpts) -> Result<(), MediaError>,
      F: FnMut(DrawingPass, &Path) -> Result<(), MediaError> {
    let mut first = true;
    run_passes(subject, construction, output_dir, options, |prompt, dest, opts| {
        let mut opts = opts.clone();
        if first { opts.reference_image_paths.insert(0, pose.to_owned()); }
        generate(prompt, dest, &opts)?;
        first = false;
        Ok(())
    }, on_pass)
}

fn run_passes<G, F>(subject: &str, construction: &str, output_dir: &Path, options: &ImageGenOpts,
    mut generate: G, mut on_pass: F) -> Result<Vec<PathBuf>, MediaError>
where G: FnMut(&str, &Path, &ImageGenOpts) -> Result<(), MediaError>,
      F: FnMut(DrawingPass, &Path) -> Result<(), MediaError> {
    if subject.trim().is_empty() || construction.trim().is_empty() || options.init_image_path.is_some() || options.mask_image_path.is_some() {
        return Err(MediaError::EngineFailed { engine: "illustration".into(),
            detail: "requires a nonempty subject and reference editing, not img2img/inpainting".into() });
    }
    // sd-cli runs from its engine directory. Relative outputs/references would
    // otherwise be interpreted there instead of in the caller's workspace.
    let output_dir = std::path::absolute(output_dir)?;
    std::fs::create_dir(&output_dir)?;
    std::fs::write(output_dir.join("subject.txt"), subject)?;
    std::fs::write(output_dir.join("construction.txt"), construction)?;
    let mut outputs: Vec<PathBuf> = Vec::new();
    for (index, pass) in [DrawingPass::Skeleton, DrawingPass::Volumes, DrawingPass::Contours,
        DrawingPass::Details, DrawingPass::Final].into_iter().enumerate() {
        if crate::media_cancel_requested() { return Err(MediaError::Cancelled); }
        let mut opts = options.clone();
        // Most recent pass is reference 1. User-supplied references follow it.
        if let Some(previous) = outputs.last() {
            opts.reference_image_paths.insert(0, previous.clone());
        }
        // Put the present drawing task first. A finished-art brief first caused
        // Klein to skip construction and render a detailed scene immediately.
        let scene = if matches!(pass, DrawingPass::Skeleton | DrawingPass::Volumes) {
            construction.to_owned()
        } else {
            // The reference now carries the construction. Repeating its prose
            // also repeats temporary omissions (blank head, no scenery, etc.),
            // which can prevent the final subject from ever being completed.
            // The caller's single-frame subject carries lasting requirements.
            format!("Original user request, for subject identity and details: {subject}\nThe reference establishes the depicted instant, pose, framing, scale and contacts. Preserve those spatial relationships, but develop unfinished placeholders into the requested subjects. Instructions to omit faces, clothing, accessories, scenery or color applied ONLY to the earlier construction stages and must not suppress requested details in this stage. If the original request describes successive actions, depict ONLY the single instant established by the reference. Do not add another instance of a subject to show an earlier or later action. Do not combine successive poses into one body. This is one keyframe, not a storyboard or a complete animation.")
        };
        let prompt = format!("{}\n\nScene to construct: {scene}\n\nRender ONLY the current {} stage. The first reference after the initial pass is the drawing to develop. Single image, no montage, labels or tutorial panels.", pass.instruction(), pass.name());
        let dest = output_dir.join(format!("{index}-{}.png", pass.name()));
        std::fs::write(output_dir.join(format!("{index}-{}-prompt.txt", pass.name())), &prompt)?;
        generate(&prompt, &dest, &opts)?;
        image::open(&dest).map_err(|error| MediaError::EngineFailed {
            engine: "illustration".into(), detail: format!("invalid {} output: {error}", pass.name()) })?;
        on_pass(pass, &dest)?;
        outputs.push(dest);
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp_dir() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!("aos-passes-{}-{unique}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()))
    }
    #[test]
    fn each_pass_consumes_previous_image_and_publishes_in_order() {
        let dir = temp_dir();
        let mut calls = 0;
        let mut published = Vec::new();
        let last_published = std::cell::RefCell::new(None::<PathBuf>);
        let outputs = run_passes("a seated gardener", "seated pose", &dir, &ImageGenOpts::default(),
            |prompt, dest, opts| {
                if calls < 2 {
                    assert!(prompt.contains("seated pose"));
                    assert!(!prompt.contains("gardener"));
                } else {
                    assert!(prompt.contains("a seated gardener"));
                    assert!(!prompt.contains("seated pose"));
                    assert!(prompt.contains("applied ONLY to the earlier construction stages"));
                    assert!(prompt.contains("ONLY the single instant"));
                }
                assert_eq!(opts.reference_image_paths.len(), usize::from(calls > 0));
                if calls > 0 {
                    assert_eq!(Some(&opts.reference_image_paths[0]), last_published.borrow().as_ref());
                    assert!(opts.reference_image_paths[0].is_file());
                }
                image::RgbImage::new(8, 8).save(dest).unwrap();
                calls += 1;
                Ok(())
            }, |pass, path| {
                assert!(path.is_file());
                *last_published.borrow_mut() = Some(path.to_path_buf());
                published.push(pass);
                Ok(())
            }).unwrap();
        assert_eq!(outputs.len(), 5);
        assert_eq!(published[0], DrawingPass::Skeleton);
        assert_eq!(published[4], DrawingPass::Final);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn pose_bootstrap_is_not_reintroduced_into_finishing_passes() {
        let dir = temp_dir();
        let guide = std::path::absolute("pose-guide.png").unwrap();
        let mut previous = guide.clone();
        let outputs = run_pose_passes("a figure", "a pose", &guide, &dir,
            &ImageGenOpts::default(), |_, dest, opts| {
                assert_eq!(opts.reference_image_paths, vec![previous.clone()]);
                image::RgbImage::new(8, 8).save(dest).unwrap();
                previous = dest.to_owned();
                Ok(())
            }, |_, _| Ok(())).unwrap();
        assert_eq!(outputs.len(), 5);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn supplied_pose_reference_follows_previous_pass_without_accumulation() {
        let dir = temp_dir();
        let guide = std::path::absolute("pose-guide.png").unwrap();
        let opts = ImageGenOpts { reference_image_paths: vec![guide.clone()], ..Default::default() };
        let mut previous = None::<PathBuf>;
        let outputs = run_passes("a seated figure", "two arms", &dir, &opts,
            |_, dest, current| {
                let expected = match &previous {
                    Some(path) => vec![path.clone(), guide.clone()],
                    None => vec![guide.clone()],
                };
                assert_eq!(current.reference_image_paths, expected);
                image::RgbImage::new(8, 8).save(dest).unwrap();
                previous = Some(dest.to_owned());
                Ok(())
            }, |_, _| Ok(())).unwrap();
        assert_eq!(outputs.len(), 5);
        assert_eq!(opts.reference_image_paths, vec![guide]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn relative_output_publishes_absolute_reference_paths() {
        let name = temp_dir().file_name().unwrap().to_owned();
        let dir = PathBuf::from(name);
        let outputs = run_passes("a cat", "a quadruped", &dir, &ImageGenOpts::default(),
            |_, dest, opts| {
                assert!(dest.is_absolute());
                assert!(opts.reference_image_paths.iter().all(|p| p.is_absolute()));
                image::RgbImage::new(8, 8).save(dest).unwrap();
                Ok(())
            }, |_, path| { assert!(path.is_absolute()); Ok(()) }).unwrap();
        assert_eq!(outputs.len(), 5);
        std::fs::remove_dir_all(std::path::absolute(&dir).unwrap()).unwrap();
    }

    #[test]
    fn rejected_pass_stops_before_next_generation() {
        let dir = temp_dir();
        let mut calls = 0;
        let result = run_passes("a cat", "curled quadruped", &dir, &ImageGenOpts::default(), |_, dest, _| {
            calls += 1;
            image::RgbImage::new(8, 8).save(dest).unwrap();
            Ok(())
        }, |_, _| Err(MediaError::Cancelled));
        assert!(matches!(result, Err(MediaError::Cancelled)), "{result:?}");
        assert_eq!(calls, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
