//! Stub beauty-pass renderer — solid viewport placeholder PNG (no Blender).

use crate::png::encode_rgba8_png;

/// Output edge size for the stub beauty pass.
pub const STUB_BEAUTY_SIZE: u32 = 64;

/// Encode a tiny solid RGBA PNG (viewport capture placeholder).
///
/// Color channels are linear 0–255 for the stub only; not a color-managed path.
pub fn stub_beauty_png(r: u8, g: u8, b: u8) -> Vec<u8> {
    let w = STUB_BEAUTY_SIZE;
    let h = STUB_BEAUTY_SIZE;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for px in rgba.as_chunks_mut::<4>().0 {
        *px = [r, g, b, 255];
    }
    encode_rgba8_png(w, h, &rgba).expect("stub png encode")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_png_has_signature() {
        let png = stub_beauty_png(40, 80, 120);
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
        assert!(png.len() > 100);
    }
}
