//! Génération de fichiers (md/txt/json/csv/png/pdf) — Preview PC.9.

use image::{ImageBuffer, Rgb, RgbImage};

#[derive(Debug)]
pub enum GenError {
    Unsupported(String),
    Io(String),
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(s) | Self::Io(s) => write!(f, "{s}"),
        }
    }
}

/// Produit les octets du fichier demandé.
pub fn generate(format: &str, content: &str, title: Option<&str>) -> Result<Vec<u8>, GenError> {
    match format.to_ascii_lowercase().as_str() {
        "md" | "txt" | "csv" => Ok(content.as_bytes().to_vec()),
        "json" => {
            // Valide ou encapsule.
            if serde_json::from_str::<serde_json::Value>(content).is_ok() {
                Ok(content.as_bytes().to_vec())
            } else {
                Ok(serde_json::json!({ "text": content }).to_string().into_bytes())
            }
        }
        "png" => generate_png(content, title),
        "pdf" => generate_pdf(content, title),
        "mp3" | "wav" | "mp4" | "webm" | "audio" | "video" => Err(GenError::Unsupported(
            "génération audio/vidéo non disponible en Preview 0.1 (vague 2)".into(),
        )),
        other => Err(GenError::Unsupported(format!("format inconnu: {other}"))),
    }
}

fn generate_png(content: &str, title: Option<&str>) -> Result<Vec<u8>, GenError> {
    let w = 640u32;
    let h = 360u32;
    let mut img: RgbImage = ImageBuffer::from_pixel(w, h, Rgb([24, 28, 36]));
    // Barre titre
    for x in 0..w {
        for y in 0..40 {
            img.put_pixel(x, y, Rgb([40, 80, 140]));
        }
    }
    // « Texte » simulé : lignes proportionnelles au contenu (pas de font raster
    // complète — motif déterministe pour artefact visible).
    let label = title.unwrap_or("Agent OS");
    draw_bars(&mut img, 20, 60, label.len() as u32 * 8 + 40, 16, Rgb([220, 220, 230]));
    let lines: Vec<&str> = content.lines().take(12).collect();
    for (i, line) in lines.iter().enumerate() {
        let y = 100 + (i as u32) * 18;
        let width = (line.len() as u32 * 6).min(w - 40);
        draw_bars(&mut img, 20, y, width.max(8), 10, Rgb([180, 190, 200]));
    }
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| GenError::Io(e.to_string()))?;
    Ok(buf)
}

fn draw_bars(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, color: Rgb<u8>) {
    let (iw, ih) = (img.width(), img.height());
    for yy in y..(y + h).min(ih) {
        for xx in x..(x + w).min(iw) {
            img.put_pixel(xx, yy, color);
        }
    }
}

fn generate_pdf(content: &str, title: Option<&str>) -> Result<Vec<u8>, GenError> {
    use printpdf::*;
    let (doc, page1, layer1) =
        PdfDocument::new(title.unwrap_or("Agent OS"), Mm(210.0), Mm(297.0), "Layer 1");
    let layer = doc.get_page(page1).get_layer(layer1);
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| GenError::Io(e.to_string()))?;
    let mut y = 280.0;
    layer.use_text(
        title.unwrap_or("Document Agent OS"),
        18.0,
        Mm(20.0),
        Mm(y),
        &font,
    );
    y -= 12.0;
    for line in content.lines().take(40) {
        y -= 6.0;
        if y < 20.0 {
            break;
        }
        let clipped: String = line.chars().take(90).collect();
        layer.use_text(clipped, 11.0, Mm(20.0), Mm(y), &font);
    }
    let mut buf = Vec::new();
    doc.save(&mut std::io::BufWriter::new(&mut buf))
        .map_err(|e| GenError::Io(e.to_string()))?;
    Ok(buf)
}
