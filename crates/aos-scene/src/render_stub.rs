//! Stub beauty-pass renderer — solid viewport placeholder PNG (no Blender).

/// Output edge size for the stub beauty pass.
pub const STUB_BEAUTY_SIZE: u32 = 64;

/// Encode a tiny solid RGBA PNG (viewport capture placeholder).
///
/// Color channels are linear 0–255 for the stub only; not a color-managed path.
pub fn stub_beauty_png(r: u8, g: u8, b: u8) -> Vec<u8> {
    // Minimal uncompressed-ish PNG via raw IHDR + IDAT (store filter-none rows).
    // Hand-rolled to avoid pulling `image` into aos-scene.
    let w = STUB_BEAUTY_SIZE;
    let h = STUB_BEAUTY_SIZE;
    let mut raw = Vec::with_capacity((w as usize + 1) * h as usize * 4);
    for _y in 0..h {
        raw.push(0); // filter None
        for _x in 0..w {
            raw.extend_from_slice(&[r, g, b, 255]);
        }
    }
    let compressed = deflate_store(&raw);
    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    write_chunk(&mut png, b"IHDR", &{
        let mut d = Vec::new();
        d.extend_from_slice(&w.to_be_bytes());
        d.extend_from_slice(&h.to_be_bytes());
        d.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
        d
    });
    write_chunk(&mut png, b"IDAT", &compressed);
    write_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_chunk(out: &mut Vec<u8>, ty: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(ty);
    out.extend_from_slice(data);
    let mut crc_data = Vec::with_capacity(4 + data.len());
    crc_data.extend_from_slice(ty);
    crc_data.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
}

/// zlib wrapper around uncompressed deflate stored blocks (type 00).
fn deflate_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    // zlib header: CMF/FLG with no dict, check bits
    out.push(0x78);
    out.push(0x01);
    let mut offset = 0;
    while offset < data.len() {
        let remaining = data.len() - offset;
        let take = remaining.min(65535);
        let last = offset + take >= data.len();
        out.push(if last { 0x01 } else { 0x00 });
        let n = take as u16;
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(!n).to_le_bytes());
        out.extend_from_slice(&data[offset..offset + take]);
        offset += take;
    }
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xffff_ffff;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (!(crc & 1)).wrapping_add(1);
            crc = (crc >> 1) ^ (0xEDB88320 & mask);
        }
    }
    !crc
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
