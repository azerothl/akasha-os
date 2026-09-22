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
        // Stub always emits fixed size; width/height on the request are advisory.
        let _ = (req.width, req.height, req.pass, &req.scene);
        // When an NPR style is selected, tint the stub so DeclUI preview
        // still reflects the choice without requiring CPU/Blender.
        let png = if let Some(style) = &req.style {
            let pr = style.paper_tint[0].saturating_add(8);
            let pg = style.paper_tint[1];
            let stroke = style.stroke_rgb();
            stub_beauty_png(
                ((r as u16 + pr as u16) / 2) as u8,
                ((g as u16 + pg as u16) / 2) as u8,
                ((b as u16 + stroke[2] as u16) / 2) as u8,
            )
        } else {
            stub_beauty_png(r, g, b)
        };
        Ok(RenderOutput {
            png,
            width: STUB_BEAUTY_SIZE,
            height: STUB_BEAUTY_SIZE,
            backend_id: RenderBackendId::Stub,
            pass: RenderPassKind::Beauty,
        })
    }
}
