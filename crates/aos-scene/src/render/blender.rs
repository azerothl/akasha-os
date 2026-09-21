//! Blender beauty RenderBackend (host orchestrator — no bpy).
//!
//! Real renders invoke the optional Illustration Renderer Pack across a
//! process boundary. Default / CI path uses deterministic **mock** mode so
//! Blender need not be downloaded.

use super::backend::{
    RenderBackend, RenderBackendId, RenderError, RenderOutput, RenderPassKind, RenderRequest,
};
use super::export::AkashaSceneExport;
use super::isolate::{
    bwrap_available, resolve_adapter_py, resolve_blender_bin, resolve_pack_root, spawn_isolated,
    BlenderRunMode, BlenderSpawnPlan, DEFAULT_BLENDER_TIMEOUT_SECS,
};
use crate::png::encode_rgba8_png;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Soft cap for Blender / mock output edges.
pub const BLENDER_MAX_EDGE: u32 = 1024;

/// Default logical output when DeclUI omits path.
pub const DEFAULT_BLENDER_BEAUTY_PATH: &str = "/documents/illustrations/beauty-blender.png";

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
        let export = AkashaSceneExport::from_scene(&req.scene, w, h, req.pass.as_str())
            .map_err(RenderError::Scene)?;
        let digest = export.digest_hex().map_err(RenderError::Scene)?;

        let use_mock = match self.mode {
            BlenderRunMode::Mock => true,
            BlenderRunMode::Require => false,
            BlenderRunMode::Auto => {
                let pack = resolve_pack_root();
                let bin = resolve_blender_bin(pack.as_deref());
                let adapter_ok = pack
                    .as_ref()
                    .and_then(|p| resolve_adapter_py(p))
                    .is_some();
                bin.is_none() || !adapter_ok
            }
        };

        if use_mock {
            return mock_beauty(w, h, req.pass, &digest);
        }

        self.render_real(req, &export, w, h)
    }
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
                "illustration renderer pack not found (set AOS_ILLUSTRATION_RENDERER_PACK)"
                    .into(),
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
        let json = export.to_canonical_json().map_err(RenderError::Scene)?;
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

        let result = spawn_isolated(&plan).map_err(RenderError::Isolation)?;
        if result.exit_code != 0 {
            let _ = fs::remove_dir_all(&work_dir);
            return Err(RenderError::Isolation(format!(
                "blender exit {}: {}",
                result.exit_code,
                result.stderr_tail
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

/// Deterministic mock beauty: solid teal field + digest-tinted strip.
/// Distinct from stub (grey-blue) and CPU wireframe so DeclUI can tell modes apart.
fn mock_beauty(
    w: u32,
    h: u32,
    pass: RenderPassKind,
    digest_hex: &str,
) -> Result<RenderOutput, RenderError> {
    let tint = u8::from_str_radix(&digest_hex[0..2], 16).unwrap_or(0x40);
    let bg = match pass {
        RenderPassKind::Beauty => [24u8, 96, 88, 255],
        RenderPassKind::Wireframe => [16u8, 48, 44, 255],
    };
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let on_strip = y < 4 || (y >= h.saturating_sub(4));
            if on_strip {
                rgba[i] = tint;
                rgba[i + 1] = tint.wrapping_add(40);
                rgba[i + 2] = 200;
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
        rgba[2] = 1;
        rgba[3] = 255;
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
    use crate::scene::SceneGraph;

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
        };
        let a = backend.render(&req).unwrap();
        let b = backend.render(&req).unwrap();
        assert_eq!(a.png, b.png);
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
            })
            .expect("real blender");
        assert!(out.png.starts_with(&[0x89, 0x50, 0x4e, 0x47]));
    }
}
