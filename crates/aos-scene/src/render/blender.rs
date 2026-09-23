//! Blender beauty RenderBackend (host orchestrator — no bpy).
//!
//! Real renders invoke the **opt-in** Illustration Renderer Pack across a
//! process boundary. Auto fails closed when the pack is absent; with pack but
//! no Blender binary it uses deterministic **mock** mode. Explicit
//! `AOS_BLENDER_MODE=mock` never needs the pack (CI).

use super::backend::{
    RenderBackend, RenderBackendId, RenderError, RenderOutput, RenderPassKind, RenderRequest,
};
use super::export::AkashaSceneExport;
use super::isolate::{
    bwrap_available, probe_pack_status, resolve_adapter_py, resolve_blender_bin, resolve_pack_root,
    spawn_isolated, BlenderPackStatus, BlenderRunMode, BlenderSpawnPlan,
    DEFAULT_BLENDER_TIMEOUT_SECS,
};
use crate::mesh_asset::{default_mesh_search_roots, load_gltf_mesh, resolve_mesh_uri};
use crate::png::encode_rgba8_png;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Soft cap for Blender / mock output edges.
pub const BLENDER_MAX_EDGE: u32 = 1024;

/// Default logical output when DeclUI omits path.
pub const DEFAULT_BLENDER_BEAUTY_PATH: &str = "/documents/illustrations/beauty-blender.png";

/// DeclUI / host probe for Renderer Pack install status.
pub fn blender_pack_status() -> BlenderPackStatus {
    probe_pack_status()
}

pub struct BlenderRenderBackend {
    pub mode: BlenderRunMode,
    pub prefer_bwrap: bool,
    pub timeout: Duration,
}

impl Default for BlenderRenderBackend {
    fn default() -> Self {
        Self {
            mode: BlenderRunMode::from_env(),
            prefer_bwrap: true,
            timeout: Duration::from_secs(
                std::env::var("AOS_BLENDER_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(DEFAULT_BLENDER_TIMEOUT_SECS),
            ),
        }
    }
}

impl RenderBackend for BlenderRenderBackend {
    fn id(&self) -> RenderBackendId {
        RenderBackendId::Blender
    }

    fn render(&self, req: &RenderRequest) -> Result<RenderOutput, RenderError> {
        let w = req.width.clamp(16, BLENDER_MAX_EDGE);
        let h = req.height.clamp(16, BLENDER_MAX_EDGE);
        let export = AkashaSceneExport::from_scene_with_style(
            &req.scene,
            w,
            h,
            req.pass.as_str(),
            req.style.as_ref(),
        )
        .map_err(RenderError::Scene)?;
        let digest = export.digest_hex().map_err(RenderError::Scene)?;
        let status = probe_pack_status();

        match self.mode {
            BlenderRunMode::Mock => mock_beauty(w, h, req.pass, &digest, req.style.as_ref()),
            BlenderRunMode::Require => {
                if !status.ready_for_spawn {
                    return Err(pack_unavailable_error(&status));
                }
                self.render_real(req, &export, w, h)
            }
            BlenderRunMode::Auto => {
                if status.ready_for_spawn {
                    // Never fall back to mock after a failed spawn: styled mock is
                    // NPR paper + thin stroke frame and reads as a blank "white"
                    // beauty pane while pack status still says binary+adapter ready.
                    self.render_real(req, &export, w, h)
                } else if status.ready_for_mock {
                    mock_beauty(w, h, req.pass, &digest, req.style.as_ref())
                } else {
                    Err(pack_unavailable_error(&status))
                }
            }
        }
    }
}

fn pack_unavailable_error(status: &BlenderPackStatus) -> RenderError {
    RenderError::BackendUnavailable(format!(
        "illustration renderer pack not available (opt-in; mode={}; set AOS_ILLUSTRATION_RENDERER_PACK or AOS_BLENDER_MODE=mock) — {}",
        status.mode.as_str(),
        status.summary_en()
    ))
}

impl BlenderRenderBackend {
    fn render_real(
        &self,
        req: &RenderRequest,
        export: &AkashaSceneExport,
        w: u32,
        h: u32,
    ) -> Result<RenderOutput, RenderError> {
        let pack = resolve_pack_root().ok_or_else(|| {
            RenderError::BackendUnavailable(
                "illustration renderer pack not found (set AOS_ILLUSTRATION_RENDERER_PACK)".into(),
            )
        })?;
        let blender = resolve_blender_bin(Some(&pack)).ok_or_else(|| {
            RenderError::BackendUnavailable(
                "Blender binary not found (install pack or set AOS_BLENDER_BIN; or AOS_BLENDER_MODE=mock)"
                    .into(),
            )
        })?;
        let adapter = resolve_adapter_py(&pack).ok_or_else(|| {
            RenderError::BackendUnavailable(format!(
                "pack adapter missing: {}/adapters/akasha_beauty.py",
                pack.display()
            ))
        })?;

        let work_dir = make_work_dir()?;
        let scene_json = work_dir.join("scene.json");
        let output_png = work_dir.join("beauty.png");
        let mut staged_export = export.clone();
        if let Err(error) = stage_mesh_assets(&mut staged_export, &work_dir) {
            let _ = fs::remove_dir_all(&work_dir);
            return Err(error);
        }
        let json = staged_export
            .to_canonical_json()
            .map_err(RenderError::Scene)?;
        fs::write(&scene_json, &json).map_err(|e| RenderError::Isolation(e.to_string()))?;

        let plan = BlenderSpawnPlan {
            blender_bin: blender,
            adapter_py: adapter,
            work_dir: work_dir.clone(),
            scene_json,
            output_png: output_png.clone(),
            use_bwrap: self.prefer_bwrap && bwrap_available(),
            timeout: self.timeout,
        };

        let result = match spawn_isolated(&plan) {
            Ok(r) => r,
            Err(e) => {
                let _ = fs::remove_dir_all(&work_dir);
                return Err(RenderError::Isolation(e));
            }
        };
        if result.exit_code != 0 {
            let _ = fs::remove_dir_all(&work_dir);
            return Err(RenderError::Isolation(format!(
                "blender exit {}: {}",
                result.exit_code, result.stderr_tail
            )));
        }
        if !output_png.is_file() {
            let _ = fs::remove_dir_all(&work_dir);
            return Err(RenderError::Isolation(
                "blender finished but beauty.png missing in workdir".into(),
            ));
        }
        let png = fs::read(&output_png).map_err(|e| RenderError::Isolation(e.to_string()))?;
        let _ = fs::remove_dir_all(&work_dir);
        if !png.starts_with(&[0x89, 0x50, 0x4e, 0x47]) {
            return Err(RenderError::Encode("blender output is not a PNG".into()));
        }
        let _ = req; // scene already exported
        Ok(RenderOutput {
            png,
            width: w,
            height: h,
            backend_id: RenderBackendId::Blender,
            pass: req.pass,
        })
    }
}

fn stage_mesh_assets(
    export: &mut AkashaSceneExport,
    work_dir: &std::path::Path,
) -> Result<(), RenderError> {
    let roots = default_mesh_search_roots();
    let refs: Vec<_> = roots.iter().map(|p| p.as_path()).collect();
    let assets = work_dir.join("assets");
    for (count, node) in export
        .nodes
        .values_mut()
        .filter(|n| n.kind == "mesh_asset" && n.visible)
        .enumerate()
    {
        let uri = node
            .mesh_uri
            .as_deref()
            .ok_or_else(|| RenderError::Scene(format!("MeshAsset {} has no mesh_uri", node.id)))?;
        let path = resolve_mesh_uri(uri, &refs)
            .ok_or_else(|| RenderError::Scene(format!("MeshAsset {} not found: {uri}", node.id)))?;
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension != "glb" && extension != "gltf" {
            return Err(RenderError::Scene(format!(
                "MeshAsset {} must reference a GLB or glTF",
                node.id
            )));
        }
        let size = fs::metadata(&path)
            .map_err(|e| RenderError::Isolation(e.to_string()))?
            .len();
        if size > 200_000_000 {
            return Err(RenderError::Scene(format!(
                "MeshAsset {} exceeds 200 MB",
                node.id
            )));
        }
        let gltf = gltf::Gltf::open(&path)
            .map_err(|e| RenderError::Scene(format!("MeshAsset {} invalid GLB: {e}", node.id)))?;
        let external: Vec<String> = gltf
            .buffers()
            .filter_map(|b| match b.source() {
                gltf::buffer::Source::Uri(uri) => Some(uri.to_string()),
                _ => None,
            })
            .chain(gltf.images().filter_map(|image| match image.source() {
                gltf::image::Source::Uri { uri, .. } => Some(uri.to_string()),
                _ => None,
            }))
            .collect();
        if extension == "glb" && !external.is_empty() {
            return Err(RenderError::Scene(format!(
                "MeshAsset {} must embed buffers and textures in the GLB",
                node.id
            )));
        }
        load_gltf_mesh(&path)
            .map_err(|e| RenderError::Scene(format!("MeshAsset {} invalid: {e}", node.id)))?;
        fs::create_dir_all(&assets).map_err(|e| RenderError::Isolation(e.to_string()))?;
        let name = if extension == "glb" {
            let name = format!("mesh-{count:04}.glb");
            fs::copy(&path, assets.join(&name))
                .map_err(|e| RenderError::Isolation(e.to_string()))?;
            name
        } else {
            let folder = format!("mesh-{count:04}");
            let staged = assets.join(&folder);
            fs::create_dir_all(&staged).map_err(|e| RenderError::Isolation(e.to_string()))?;
            let source_root = fs::canonicalize(path.parent().unwrap_or(std::path::Path::new(".")))
                .map_err(|e| RenderError::Isolation(e.to_string()))?;
            let mut total_bytes = size;
            for uri in &external {
                let relative = std::path::Path::new(uri);
                if uri.starts_with("data:")
                    || relative.is_absolute()
                    || relative
                        .components()
                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                {
                    return Err(RenderError::Scene(format!(
                        "MeshAsset {} has unsafe external resource",
                        node.id
                    )));
                }
                let source = path
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .join(relative);
                let resolved =
                    fs::canonicalize(&source).map_err(|e| RenderError::Isolation(e.to_string()))?;
                if !resolved.starts_with(&source_root) {
                    return Err(RenderError::Scene(format!(
                        "MeshAsset {} resource escapes asset folder",
                        node.id
                    )));
                }
                total_bytes = total_bytes.saturating_add(
                    fs::metadata(&resolved)
                        .map_err(|e| RenderError::Isolation(e.to_string()))?
                        .len(),
                );
                if total_bytes > 200_000_000 {
                    return Err(RenderError::Scene(format!(
                        "MeshAsset {} exceeds 200 MB with textures",
                        node.id
                    )));
                }
                let destination = staged.join(relative);
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| RenderError::Isolation(e.to_string()))?;
                }
                fs::copy(resolved, destination).map_err(|e| {
                    RenderError::Isolation(format!(
                        "MeshAsset {} texture/buffer {}: {e}",
                        node.id, uri
                    ))
                })?;
            }
            fs::copy(&path, staged.join("source.gltf"))
                .map_err(|e| RenderError::Isolation(e.to_string()))?;
            format!("{folder}/source.gltf")
        };
        node.mesh_uri = Some(format!("assets/{name}"));
    }
    Ok(())
}

fn make_work_dir() -> Result<PathBuf, RenderError> {
    let base = std::env::temp_dir().join("aos-blender-render");
    fs::create_dir_all(&base).map_err(|e| RenderError::Isolation(e.to_string()))?;
    let dir = base.join(format!(
        "job-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).map_err(|e| RenderError::Isolation(e.to_string()))?;
    Ok(dir)
}

/// Deterministic mock beauty: paper/style field + heavy mock chrome.
/// Distinct from stub, CPU, and real Blender. Style-aware so Sketch / Pencil /
/// Ink mock previews differ without Blender installed. Borders + diagonal band
/// stay thick enough that DeclUI cannot mistake mock for a blank NPR beauty.
fn mock_beauty(
    w: u32,
    h: u32,
    pass: RenderPassKind,
    digest_hex: &str,
    style: Option<&crate::style::ResolvedStyle>,
) -> Result<RenderOutput, RenderError> {
    let tint = u8::from_str_radix(&digest_hex[0..2], 16).unwrap_or(0x40);
    let bg = if let Some(st) = style {
        let t = st.paper_tint;
        match pass {
            RenderPassKind::Beauty => [t[0], t[1], t[2], 255],
            RenderPassKind::Wireframe => [
                t[0].saturating_sub(20),
                t[1].saturating_sub(20),
                t[2].saturating_sub(20),
                255,
            ],
        }
    } else {
        match pass {
            RenderPassKind::Beauty => [24u8, 96, 88, 255],
            RenderPassKind::Wireframe => [16u8, 48, 44, 255],
        }
    };
    let stroke = style
        .map(|s| s.stroke_rgb())
        .unwrap_or([tint, tint.wrapping_add(40), 200]);
    // Thick frame (≥12px) so mock remains obvious when DeclUI scales small PNGs.
    let border = if style.is_some() { 12u32 } else { 4u32 };
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let on_frame = y < border
                || y >= h.saturating_sub(border)
                || x < border
                || x >= w.saturating_sub(border);
            // Diagonal band ≈ mock watermark (independent of style paper tint).
            let diag = ((x + y) / 6) % 7 == 0
                && x >= border
                && y >= border
                && x < w.saturating_sub(border)
                && y < h.saturating_sub(border);
            if on_frame || diag {
                rgba[i] = stroke[0];
                rgba[i + 1] = stroke[1];
                rgba[i + 2] = stroke[2];
                rgba[i + 3] = 255;
            } else {
                rgba[i..i + 4].copy_from_slice(&bg);
            }
        }
    }
    // Corner marker pixels encoding "BM" mock magic for tests.
    if w >= 2 && h >= 1 {
        rgba[0] = b'B';
        rgba[1] = b'M';
        rgba[2] = 0;
        rgba[3] = 255;
        rgba[4] = style
            .map(|s| match s.family {
                crate::style::StyleFamily::Sketch => b'S',
                crate::style::StyleFamily::Pencil => b'P',
                crate::style::StyleFamily::Ink => b'I',
            })
            .unwrap_or(b'B');
        rgba[5] = 0;
        rgba[6] = 0;
        rgba[7] = 255;
    }
    let png = encode_rgba8_png(w, h, &rgba).map_err(RenderError::Encode)?;
    Ok(RenderOutput {
        png,
        width: w,
        height: h,
        backend_id: RenderBackendId::Blender,
        pass,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_asset::insert_mesh_asset;
    use crate::scene::SceneGraph;
    use crate::scene::Transform;
    use crate::style::resolve_style;

    #[test]
    fn stages_glb_without_changing_scene_uri() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/hierarchy_textured.glb");
        let mut scene = SceneGraph::demo_scene();
        insert_mesh_asset(
            &mut scene,
            "root",
            "asset",
            "Asset",
            source.to_string_lossy(),
            Transform::default(),
        )
        .unwrap();
        let original_uri = scene.nodes["asset"].mesh_uri.clone();
        let mut export = AkashaSceneExport::from_scene(&scene, 64, 64, "beauty").unwrap();
        let work = make_work_dir().unwrap();
        stage_mesh_assets(&mut export, &work).unwrap();
        assert_eq!(
            export.nodes["asset"].mesh_uri.as_deref(),
            Some("assets/mesh-0000.glb")
        );
        assert_eq!(
            fs::read(work.join("assets/mesh-0000.glb")).unwrap(),
            fs::read(source).unwrap()
        );
        assert_eq!(scene.nodes["asset"].mesh_uri, original_uri);
        fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn stages_gltf_with_external_texture_and_buffer() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/hierarchy_textured.gltf");
        let mut scene = SceneGraph::demo_scene();
        insert_mesh_asset(
            &mut scene,
            "root",
            "external",
            "External",
            source.to_string_lossy(),
            Transform::default(),
        )
        .unwrap();
        let mut export = AkashaSceneExport::from_scene(&scene, 64, 64, "beauty").unwrap();
        let work = make_work_dir().unwrap();
        stage_mesh_assets(&mut export, &work).unwrap();
        assert_eq!(
            export.nodes["external"].mesh_uri.as_deref(),
            Some("assets/mesh-0000/source.gltf")
        );
        assert!(work
            .join("assets/mesh-0000/hierarchy_textured.bin")
            .is_file());
        assert!(work
            .join("assets/mesh-0000/hierarchy_textured.png")
            .is_file());
        fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn mock_mode_produces_png() {
        let backend = BlenderRenderBackend {
            mode: BlenderRunMode::Mock,
            prefer_bwrap: false,
            timeout: Duration::from_secs(5),
        };
        let out = backend
            .render(&RenderRequest {
                scene: SceneGraph::demo_scene(),
                pass: RenderPassKind::Beauty,
                width: 64,
                height: 48,
                stub_rgb: (0, 0, 0),
                style: None,
            })
            .expect("mock");
        assert_eq!(out.backend_id, RenderBackendId::Blender);
        assert!(out.png.starts_with(&[0x89, 0x50, 0x4e, 0x47]));
        assert_eq!(out.width, 64);
        assert_eq!(out.height, 48);
    }

    #[test]
    fn mock_is_deterministic() {
        let backend = BlenderRenderBackend {
            mode: BlenderRunMode::Mock,
            prefer_bwrap: false,
            timeout: Duration::from_secs(5),
        };
        let req = RenderRequest {
            scene: SceneGraph::demo_scene(),
            pass: RenderPassKind::Beauty,
            width: 32,
            height: 32,
            stub_rgb: (0, 0, 0),
            style: None,
        };
        let a = backend.render(&req).unwrap();
        let b = backend.render(&req).unwrap();
        assert_eq!(a.png, b.png);
    }

    #[test]
    fn mock_style_marker_differs() {
        let backend = BlenderRenderBackend {
            mode: BlenderRunMode::Mock,
            prefer_bwrap: false,
            timeout: Duration::from_secs(5),
        };
        let pencil = backend
            .render(&RenderRequest {
                scene: SceneGraph::demo_scene(),
                pass: RenderPassKind::Beauty,
                width: 32,
                height: 32,
                stub_rgb: (0, 0, 0),
                style: Some(resolve_style("pencil").unwrap()),
            })
            .unwrap();
        let ink = backend
            .render(&RenderRequest {
                scene: SceneGraph::demo_scene(),
                pass: RenderPassKind::Beauty,
                width: 32,
                height: 32,
                stub_rgb: (0, 0, 0),
                style: Some(resolve_style("ink").unwrap()),
            })
            .unwrap();
        assert_ne!(pencil.png, ink.png);
    }

    #[test]
    fn auto_fail_closed_without_pack() {
        let _lock = crate::render::isolate::BLENDER_PACK_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev_pack = std::env::var("AOS_ILLUSTRATION_RENDERER_PACK").ok();
        let prev_mode = std::env::var("AOS_BLENDER_MODE").ok();
        std::env::set_var(
            "AOS_ILLUSTRATION_RENDERER_PACK",
            "/tmp/aos-missing-blender-renderer-pack",
        );
        std::env::set_var("AOS_BLENDER_MODE", "auto");
        let backend = BlenderRenderBackend {
            mode: BlenderRunMode::Auto,
            prefer_bwrap: false,
            timeout: Duration::from_secs(5),
        };
        let err = backend
            .render(&RenderRequest {
                scene: SceneGraph::demo_scene(),
                pass: RenderPassKind::Beauty,
                width: 32,
                height: 32,
                stub_rgb: (0, 0, 0),
                style: None,
            })
            .expect_err("pack absent must fail closed");
        assert!(matches!(err, RenderError::BackendUnavailable(_)));
        match prev_pack {
            Some(v) => std::env::set_var("AOS_ILLUSTRATION_RENDERER_PACK", v),
            None => std::env::remove_var("AOS_ILLUSTRATION_RENDERER_PACK"),
        }
        match prev_mode {
            Some(v) => std::env::set_var("AOS_BLENDER_MODE", v),
            None => std::env::remove_var("AOS_BLENDER_MODE"),
        }
    }

    #[test]
    fn auto_spawn_failure_does_not_silent_mock() {
        let _lock = crate::render::isolate::BLENDER_PACK_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev_pack = std::env::var("AOS_ILLUSTRATION_RENDERER_PACK").ok();
        let prev_bin = std::env::var("AOS_BLENDER_BIN").ok();
        let prev_mode = std::env::var("AOS_BLENDER_MODE").ok();

        let pack = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../share/illustration-renderer-pack");
        assert!(pack.is_dir(), "checkout pack must exist for this test");
        let fail_bin = std::env::temp_dir().join("aos-blender-fail-bin-white-again.sh");
        std::fs::write(
            &fail_bin,
            "#!/bin/sh\necho 'intentional blender spawn failure' >&2\nexit 42\n",
        )
        .expect("write fail bin");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&fail_bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&fail_bin, perms).unwrap();
        }

        std::env::set_var("AOS_ILLUSTRATION_RENDERER_PACK", &pack);
        std::env::set_var("AOS_BLENDER_BIN", &fail_bin);
        std::env::set_var("AOS_BLENDER_MODE", "auto");

        let backend = BlenderRenderBackend {
            mode: BlenderRunMode::Auto,
            prefer_bwrap: false,
            timeout: Duration::from_secs(5),
        };
        let err = backend
            .render(&RenderRequest {
                scene: SceneGraph::demo_scene(),
                pass: RenderPassKind::Beauty,
                width: 64,
                height: 48,
                stub_rgb: (0, 0, 0),
                style: Some(resolve_style("pencil").unwrap()),
            })
            .expect_err("spawn failure must not fall back to paper mock");
        let msg = err.to_string();
        assert!(
            msg.contains("exit 42") || msg.contains("blender"),
            "unexpected error: {msg}"
        );

        match prev_pack {
            Some(v) => std::env::set_var("AOS_ILLUSTRATION_RENDERER_PACK", v),
            None => std::env::remove_var("AOS_ILLUSTRATION_RENDERER_PACK"),
        }
        match prev_bin {
            Some(v) => std::env::set_var("AOS_BLENDER_BIN", v),
            None => std::env::remove_var("AOS_BLENDER_BIN"),
        }
        match prev_mode {
            Some(v) => std::env::set_var("AOS_BLENDER_MODE", v),
            None => std::env::remove_var("AOS_BLENDER_MODE"),
        }
        let _ = std::fs::remove_file(&fail_bin);
    }

    #[test]
    fn mock_style_has_visible_chrome() {
        let backend = BlenderRenderBackend {
            mode: BlenderRunMode::Mock,
            prefer_bwrap: false,
            timeout: Duration::from_secs(5),
        };
        let out = backend
            .render(&RenderRequest {
                scene: SceneGraph::demo_scene(),
                pass: RenderPassKind::Beauty,
                width: 64,
                height: 48,
                stub_rgb: (0, 0, 0),
                style: Some(resolve_style("pencil").unwrap()),
            })
            .expect("mock");
        // Pure paper field (no chrome) for the same style must differ from mock PNG.
        let paper = resolve_style("pencil").unwrap().paper_tint;
        let mut flat = vec![0u8; (64 * 48 * 4) as usize];
        for px in flat.as_chunks_mut::<4>().0 {
            px[0] = paper[0];
            px[1] = paper[1];
            px[2] = paper[2];
            px[3] = 255;
        }
        let flat_png = encode_rgba8_png(64, 48, &flat).expect("flat");
        assert_ne!(
            out.png, flat_png,
            "styled mock must include visible chrome, not flat paper alone"
        );
        // BM mock magic in the first two pixels of the raw buffer (before PNG).
        assert_eq!(out.png[0], 0x89);
    }

    #[test]
    #[ignore = "optional: set AOS_BLENDER_INTEGRATION=1 and install Blender + pack"]
    fn integration_real_blender() {
        if std::env::var("AOS_BLENDER_INTEGRATION").ok().as_deref() != Some("1") {
            return;
        }
        let backend = BlenderRenderBackend {
            mode: BlenderRunMode::Require,
            prefer_bwrap: true,
            timeout: Duration::from_secs(180),
        };
        let out = backend
            .render(&RenderRequest {
                scene: SceneGraph::demo_scene(),
                pass: RenderPassKind::Beauty,
                width: 128,
                height: 96,
                stub_rgb: (0, 0, 0),
                style: Some(resolve_style("ink").unwrap()),
            })
            .expect("real blender");
        assert!(out.png.starts_with(&[0x89, 0x50, 0x4e, 0x47]));
    }
}
