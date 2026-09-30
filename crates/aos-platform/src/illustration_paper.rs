//! Paper in space, bounded: two sheets, a turning page, a rising cut-out, a cast shadow.

use image::{ImageBuffer, Rgb, RgbImage};

pub fn render_paper_png(width: u32, height: u32, t: f32) -> Result<Vec<u8>, String> {
    let w = width.max(64);
    let h = height.max(64);
    let t = t.clamp(0.0, 1.0);
    let room = Rgb([232, 220, 196]);
    let sheet = Rgb([248, 241, 226]);
    let ink = Rgb([30, 22, 48]);
    let shadow = Rgb([90, 70, 50]);
    let mut img: RgbImage = ImageBuffer::from_pixel(w, h, room);

    fill_rect(&mut img, 0.08, 0.62, 0.84, 0.08, shadow, 0.35);
    let shear = (t * std::f32::consts::PI).sin() * 0.35;
    fill_sheet(&mut img, 0.12, 0.18, 0.76, 0.55, shear, sheet);
    stroke_rect(&mut img, 0.12, 0.18, 0.76, 0.55, ink);

    let rise = t * 0.22;
    let cy = 0.58 - rise;
    fill_ellipse(&mut img, 0.62, cy, 0.12, 0.08, ink);
    fill_rect(&mut img, 0.58, cy + 0.06, 0.08, 0.02, shadow, 0.4);

    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

fn fill_sheet(img: &mut RgbImage, x: f32, y: f32, w: f32, h: f32, shear: f32, color: Rgb<u8>) {
    let iw = img.width() as f32;
    let ih = img.height() as f32;
    let x0 = (x * iw) as i32;
    let y0 = (y * ih) as i32;
    let x1 = ((x + w) * iw) as i32;
    let y1 = ((y + h) * ih) as i32;
    let mid = (y0 + y1) as f32 * 0.5;
    for yy in y0..y1 {
        let shift = ((yy as f32 - mid) / ih * shear * iw) as i32;
        for xx in x0..x1 {
            put(img, xx + shift, yy, color);
        }
    }
}

fn fill_rect(img: &mut RgbImage, x: f32, y: f32, w: f32, h: f32, color: Rgb<u8>, al: f32) {
    let iw = img.width() as f32;
    let ih = img.height() as f32;
    let x0 = (x * iw) as i32;
    let y0 = (y * ih) as i32;
    let x1 = ((x + w) * iw) as i32;
    let y1 = ((y + h) * ih) as i32;
    for yy in y0..y1 {
        for xx in x0..x1 {
            blend(img, xx, yy, color, al);
        }
    }
}

fn stroke_rect(img: &mut RgbImage, x: f32, y: f32, w: f32, h: f32, color: Rgb<u8>) {
    fill_rect(img, x, y, w, 0.008, color, 1.0);
    fill_rect(img, x, y + h, w, 0.008, color, 1.0);
    fill_rect(img, x, y, 0.008, h, color, 1.0);
    fill_rect(img, x + w, y, 0.008, h, color, 1.0);
}

fn fill_ellipse(img: &mut RgbImage, cx: f32, cy: f32, rx: f32, ry: f32, color: Rgb<u8>) {
    let iw = img.width() as f32;
    let ih = img.height() as f32;
    let x0 = ((cx - rx) * iw) as i32;
    let y0 = ((cy - ry) * ih) as i32;
    let x1 = ((cx + rx) * iw) as i32;
    let y1 = ((cy + ry) * ih) as i32;
    for yy in y0..y1 {
        for xx in x0..x1 {
            let nx = (xx as f32 / iw - cx) / rx;
            let ny = (yy as f32 / ih - cy) / ry;
            if nx * nx + ny * ny <= 1.0 {
                put(img, xx, yy, color);
            }
        }
    }
}

fn put(img: &mut RgbImage, x: i32, y: i32, c: Rgb<u8>) {
    blend(img, x, y, c, 1.0);
}

fn blend(img: &mut RgbImage, x: i32, y: i32, c: Rgb<u8>, al: f32) {
    if x < 0 || y < 0 || x as u32 >= img.width() || y as u32 >= img.height() {
        return;
    }
    let p = *img.get_pixel(x as u32, y as u32);
    let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * al.clamp(0.0, 1.0)) as u8;
    img.put_pixel(x as u32, y as u32, Rgb([m(p[0], c[0]), m(p[1], c[1]), m(p[2], c[2])]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_turn_changes_the_frame() {
        let a = render_paper_png(64, 64, 0.05).unwrap();
        let b = render_paper_png(64, 64, 0.55).unwrap();
        assert_ne!(a, b);
    }
}
