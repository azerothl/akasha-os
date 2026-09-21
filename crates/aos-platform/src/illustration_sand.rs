//! Sand on a light table. One take, no cuts. Dark grains are sand, the paper is the glass.

use image::{ImageBuffer, Rgb, RgbImage};

pub fn render_sand_png(width: u32, height: u32, t: f32) -> Result<Vec<u8>, String> {
    let w = width.max(64);
    let h = height.max(64);
    let t = t.clamp(0.0, 1.0);
    let glass = Rgb([243, 230, 207]);
    let sand = Rgb([30, 22, 48]);
    let mut img: RgbImage = ImageBuffer::from_pixel(w, h, glass);
    for y in 0..h {
        for x in 0..w {
            if grain_on(x, y, w, h, t) {
                img.put_pixel(x, y, sand);
            }
        }
    }
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn grain_on(x: u32, y: u32, w: u32, h: u32, t: f32) -> bool {
    let nx = x as f32 / w as f32;
    let ny = y as f32 / h as f32;
    let n = hash(x, y);
    let sprinkle = (n % 1000) as f32 / 1000.0;
    if t < 0.42 {
        let grow = t / 0.42;
        let dx = nx - 0.5;
        let dy = ny - 0.42;
        let r = 0.08 + grow * 0.34;
        dx * dx + dy * dy < r * r && sprinkle < 0.55 + grow * 0.2
    } else if t < 0.72 {
        let wipe = (t - 0.42) / 0.30;
        let band = ny > 0.15 + wipe * 0.7;
        let dx = nx - 0.5;
        let dy = ny - 0.42;
        let poured = dx * dx + dy * dy < 0.42 * 0.42 && sprinkle < 0.72;
        poured && band
    } else {
        let sweep = (t - 0.72) / 0.28;
        let clear = nx + ny * 0.4 < sweep * 1.5;
        if clear {
            return false;
        }
        let dx = nx - 0.62;
        let dy = ny - 0.55;
        dx * dx + dy * dy < 0.18 * 0.18 && sprinkle < 0.8
    }
}

fn hash(x: u32, y: u32) -> u32 {
    let mut n = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263))
        .wrapping_add(97);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n ^ (n >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sand_frames_differ() {
        let a = render_sand_png(64, 64, 0.1).unwrap();
        let b = render_sand_png(64, 64, 0.85).unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with(&[0x89, b'P', b'N', b'G']));
    }
}
