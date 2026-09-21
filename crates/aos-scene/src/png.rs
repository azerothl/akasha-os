//! Minimal RGBA PNG encoder (no external image crate).

/// Encode an RGBA8 image as a PNG (filter-none + zlib stored blocks).
pub fn encode_rgba8_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| "image dimensions overflow".to_string())?;
    if rgba.len() != expected {
        return Err(format!(
            "rgba len {} != width*height*4 ({expected})",
            rgba.len()
        ));
    }
    let mut raw = Vec::with_capacity((width as usize + 1) * height as usize * 4);
    for y in 0..height as usize {
        raw.push(0); // filter None
        let row = y * width as usize * 4;
        raw.extend_from_slice(&rgba[row..row + width as usize * 4]);
    }
    let compressed = deflate_store(&raw);
    let mut png = Vec::new();
    png.extend_from_slice(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    write_chunk(&mut png, b"IHDR", &{
        let mut d = Vec::new();
        d.extend_from_slice(&width.to_be_bytes());
        d.extend_from_slice(&height.to_be_bytes());
        d.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
        d
    });
    write_chunk(&mut png, b"IDAT", &compressed);
    write_chunk(&mut png, b"IEND", &[]);
    Ok(png)
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
    fn encodes_tiny_png() {
        let png = encode_rgba8_png(2, 2, &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255])
            .expect("png");
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    }
}
