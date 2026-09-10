//! Designer / CI screenshots via egui `ViewportCommand::Screenshot`.
//!
//! Set `AOS_UI_SCREENSHOT_DIR` to a writable directory; the app captures:
//! 1. Rail with painted icons (FR)
//! 2. Settings → Police de l'interface (default font + live preview)
//! 3. Same panel with Inter selected (preview updates)

use crate::prefs::save_preferences;
use crate::{Tab, UiApp};
use eframe::egui::{self, ColorImage, Event, UserData};
use std::path::{Path, PathBuf};

pub fn screenshot_dir_from_env() -> Option<PathBuf> {
    std::env::var("AOS_UI_SCREENSHOT_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

pub struct UiScreenshotHarness {
    dir: PathBuf,
    /// Settle frames before requesting the next capture.
    settle_left: u32,
    step: u8,
    waiting: bool,
}

impl UiScreenshotHarness {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            settle_left: 4,
            step: 0,
            waiting: false,
        }
    }

    pub fn drain_events(&mut self, ctx: &egui::Context) {
        let shots: Vec<(String, ColorImage)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|ev| {
                    if let Event::Screenshot { image, user_data, .. } = ev {
                        user_data
                            .data
                            .as_ref()
                            .and_then(|d| d.downcast_ref::<String>())
                            .map(|tag| (tag.clone(), image.as_ref().clone()))
                    } else {
                        None
                    }
                })
                .collect()
        });
        for (tag, image) in shots {
            let path = self.dir.join(format!("{tag}.png"));
            if let Err(err) = save_png(&path, &image) {
                eprintln!("AOS_UI_SCREENSHOT_DIR: failed to write {}: {err}", path.display());
            } else {
                eprintln!("AOS_UI_SCREENSHOT_DIR: wrote {}", path.display());
            }
            self.waiting = false;
        }
    }

    pub fn tick(&mut self, app: &mut UiApp, ctx: &egui::Context) {
        if self.waiting {
            return;
        }
        if self.settle_left > 0 {
            self.settle_left -= 1;
            return;
        }

        match self.step {
            0 => {
                app.tab = Tab::Chat;
                self.request(ctx, "01-rail-painted-icons-fr");
                self.step = 1;
                self.settle_left = 3;
            }
            1 => {
                app.tab = Tab::Settings;
                app.settings_ui.section = "me".into();
                app.prefs.language = "fr".into();
                self.request(ctx, "02-settings-police-interface-default-fr");
                self.step = 2;
                self.settle_left = 3;
            }
            2 => {
                app.prefs.ui_font = "inter".into();
                save_preferences(&app.prefs);
                self.request(ctx, "03-settings-police-interface-inter-fr");
                self.step = 3;
                self.settle_left = 3;
            }
            _ => {
                eprintln!("AOS_UI_SCREENSHOT_DIR: captures complete — exiting");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn request(&mut self, ctx: &egui::Context, tag: &str) {
        self.waiting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(UserData::new(
            tag.to_string(),
        )));
    }
}

fn save_png(path: &Path, image: &ColorImage) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let w = image.width() as u32;
    let h = image.height() as u32;
    if w == 0 || h == 0 {
        return Err("empty screenshot".into());
    }
    let mut buf = image::RgbaImage::new(w, h);
    for (i, px) in image.pixels.iter().enumerate() {
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        buf.put_pixel(x, y, image::Rgba([px.r(), px.g(), px.b(), px.a()]));
    }
    buf.save(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_png_writes_valid_dimensions() {
        let img = ColorImage {
            size: [2, 1],
            pixels: vec![
                egui::Color32::RED,
                egui::Color32::BLUE,
            ],
        };
        let dir = std::env::temp_dir().join("aos_ui_shot_test");
        let path = dir.join("test.png");
        save_png(&path, &img).expect("write png");
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
