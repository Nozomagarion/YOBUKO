// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A private, user-supplied image below the UI. Never reads the desktop or other windows.

use egui::{ColorImage, TextureHandle, TextureOptions, Ui};
use std::sync::mpsc::{self, Receiver};

type Loaded = Result<ColorImage, String>;

#[derive(Default)]
pub(super) struct Background {
    requested: (String, u8),
    worker: Option<Receiver<Loaded>>,
    texture: Option<TextureHandle>,
}

impl Background {
    pub fn paint(&mut self, ui: &Ui, path: &str, blur: u8) -> Result<bool, String> {
        if let Some(worker) = &self.worker {
            match worker.try_recv() {
                Ok(result) => {
                    self.worker = None;
                    self.texture = None;
                    crate::kit::liquid::set_background_image(ui.ctx(), None);
                    let pixels = std::sync::Arc::new(result?);
                    let texture = ui.ctx().load_texture("private-background", pixels.clone(), TextureOptions::LINEAR);
                    crate::kit::liquid::set_background_image(
                        ui.ctx(),
                        Some(crate::kit::liquid::BackgroundImage { id: texture.id(), pixels }),
                    );
                    self.texture = Some(texture);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if path.is_empty() {
            self.texture = None;
            crate::kit::liquid::set_background_image(ui.ctx(), None);
            self.requested = (String::new(), blur);
            return Ok(false);
        }
        if self.worker.is_none() && (self.requested.0 != path || self.requested.1 != blur) {
            self.requested = (path.to_string(), blur);
            let path = path.to_string();
            let ctx = ui.ctx().clone();
            let (send, receive) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("background-image".into())
                .spawn(move || {
                    let _ = send.send(load(&path, blur));
                    ctx.request_repaint();
                })
                .map_err(|error| error.to_string())?;
            self.worker = Some(receive);
        }
        if self.worker.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(texture) = &self.texture {
            let rect = ui.max_rect();
            // The native window stays opaque, even where the source PNG has alpha.
            ui.painter().rect_filled(rect, 0.0, crate::kit::theme::window_background().to_opaque());
            let size = texture.size_vec2();
            let scale = (rect.width() / size.x).max(rect.height() / size.y);
            let uv_size = rect.size() / (size * scale);
            let uv = egui::Rect::from_center_size(egui::pos2(0.5, 0.5), uv_size);
            ui.painter().image(texture.id(), rect, uv, egui::Color32::WHITE);
            return Ok(true);
        }
        Ok(false)
    }
}

fn load(path: &str, blur: u8) -> Loaded {
    let metadata = std::fs::metadata(path).map_err(|error| format!("Background image: {error}"))?;
    if metadata.len() > 32 * 1024 * 1024 {
        return Err("Background PNG must be smaller than 32 MiB".into());
    }
    let mut reader = image::ImageReader::open(path).map_err(|error| error.to_string())?;
    reader.set_format(image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|error| format!("Background PNG: {error}"))?;
    let image = if image.width() > 1600 || image.height() > 1600 { image.thumbnail(1600, 1600) } else { image };
    let image = image.blur(f32::from(blur.min(40))).into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_a_user_png_and_rejects_other_data() {
        let path = std::env::temp_dir().join(format!("suhina-background-test-{}.png", std::process::id()));
        image::RgbaImage::from_pixel(16, 8, image::Rgba([120, 80, 40, 128])).save(&path).unwrap();
        let loaded = load(path.to_str().unwrap(), 0).unwrap();
        assert_eq!(loaded.size, [16, 8]);
        assert_eq!(loaded.pixels[0].a(), 128);
        std::fs::write(&path, b"not a PNG").unwrap();
        assert!(load(path.to_str().unwrap(), 12).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(load(path.to_str().unwrap(), 0).is_err());
    }
}
