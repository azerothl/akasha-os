//! Stub solid-color beauty backend.

use super::backend::{
    RenderBackend, RenderBackendId, RenderError, RenderOutput, RenderPassKind, RenderRequest,
};
use crate::render_stub::{stub_beauty_png, STUB_BEAUTY_SIZE};

pub struct StubRenderBackend;

impl RenderBackend for StubRenderBackend {
    fn id(&self) -> RenderBackendId {
        RenderBackendId::Stub
    }

    fn render(&self, req: &RenderRequest) -> Result<RenderOutput, RenderError> {
        let (r, g, b) = req.stub_rgb;
        let png = stub_beauty_png(r, g, b);
        // Stub always emits fixed size; width/height on the request are advisory.
        let _ = (req.width, req.height, req.pass, &req.scene);
        Ok(RenderOutput {
            png,
            width: STUB_BEAUTY_SIZE,
            height: STUB_BEAUTY_SIZE,
            backend_id: RenderBackendId::Stub,
            pass: RenderPassKind::Beauty,
        })
    }
}
