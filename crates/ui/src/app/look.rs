// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One look for YOBUKO's three programs: the colours of the theme in use (and of the app mode)
//! go to `<data>/YOBUKO/look.json`, which Mixxx's skin and rbxport follow (`suhina_dj::look`).
//! The mode also shows here, as a thin line in its colour along the top of the window, beside
//! YOBUKO's red 呼子 seal.

use super::menu_bar::WorkspaceMode;
use super::*;
use suhina_dj::look::Look;

/// The look of `palette` (theme `theme`) in mode `mode`.
pub(crate) fn look_of(palette: &theme::Palette, theme_id: &str, mode: egui::Color32) -> Look {
    let hex = |c: egui::Color32| {
        let [r, g, b, _] = c.to_srgba_unmultiplied();
        format!("#{r:02X}{g:02X}{b:02X}")
    };
    // A see-through background is written as the colour it would be over black.
    let opaque = |c: egui::Color32| {
        let [r, g, b, _] = c.to_array();
        format!("#{r:02X}{g:02X}{b:02X}")
    };
    Look {
        format: suhina_dj::look::FORMAT,
        theme: theme_id.to_owned(),
        dark: !theme::light(palette),
        mode: hex(mode),
        ground: opaque(palette.window_background),
        surface: opaque(palette.panel_background),
        control: opaque(palette.control_background),
        control_hover: opaque(palette.control_hover),
        control_active: opaque(palette.control_active),
        line: hex(palette.border),
        line_strong: hex(palette.border_strong),
        text: hex(palette.text_primary),
        text_dim: hex(palette.text_secondary),
        hint: hex(palette.text_hint),
        inverse: hex(palette.text_inverse),
        accent: hex(palette.accent),
        selection: hex(palette.selection),
        recording: hex(palette.recording),
        meter_normal: hex(palette.meter_normal),
        meter_hot: hex(palette.meter_hot),
    }
}

impl App {
    /// Writes the look when the theme or the mode changed since it was last written. Each
    /// frame this writes only into a YOBUKO folder that is already there (Mixxx or rbxport
    /// has been used); `force` (just before either starts) makes it. Never in tests, which
    /// send their DJ requests elsewhere.
    pub(super) fn publish_look(&mut self, force: bool) -> Look {
        let mode = self.workspace_mode();
        let look = look_of(&theme::palette(), &self.settings.theme, mode.color());
        let key = (theme::palette_revision(), mode);
        if self.dj.inbox.is_some() || (!force && self.dj.look_published == Some(key)) {
            return look;
        }
        self.dj.look_published = Some(key);
        let Some(data) = dj::rbxport::user_data_dir() else {
            return look;
        };
        let folder = suhina_dj::mixxx::yobuko_dir(&data);
        if (force || folder.is_dir())
            && let Err(e) = suhina_dj::look::write(&suhina_dj::look::path(&data), &look)
        {
            eprintln!("YOBUKO: the look for Mixxx and rbxport could not be written: {e}");
        }
        look
    }

    /// The mode's colour as a thin line along the top of the window, and YOBUKO's red seal
    /// left of the mode pill (`rect`).
    pub(super) fn paint_identity(&self, ui: &Ui, mode: WorkspaceMode, pill: Rect) {
        let painter =
            ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::unique("yobuko-mode-line")));
        let screen = ui.ctx().content_rect();
        painter.rect_filled(Rect::from_min_size(screen.min, Vec2::new(screen.width(), 2.0)), 0.0, mode.color());
        let side = (pill.height() - 2.0).max(12.0);
        let seal =
            Rect::from_center_size(Pos2::new(pill.left() - 6.0 - side / 2.0, pill.center().y), Vec2::splat(side));
        let texture = seal_texture(ui.ctx());
        ui.painter().image(
            texture.id(),
            seal,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

/// YOBUKO's seal: 呼子 in cream on hanko red, drawn once from `brand/yobuko-seal.png` (the
/// interface fonts have no Japanese).
const SEAL_PNG: &[u8] = include_bytes!("../../brand/yobuko-seal.png");

fn seal_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = Id::unique("yobuko-seal-texture");
    if let Some(texture) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return texture;
    }
    let image = image::load_from_memory_with_format(SEAL_PNG, image::ImageFormat::Png)
        .map(|i| i.to_rgba8())
        .unwrap_or_else(|_| image::RgbaImage::from_pixel(1, 1, image::Rgba([0xC8, 0x37, 0x2D, 0xFF])));
    let size = [image.width() as usize, image.height() as usize];
    let colour = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    let texture = ctx.load_texture("yobuko-seal", colour, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_carries_the_themes_colours_and_the_mode() {
        let look = look_of(&theme::Palette::DARK, "yobuko", WorkspaceMode::Dj.color());
        assert!(look.dark);
        assert_eq!(look.ground, "#1D1C1A");
        assert_eq!(look.accent, "#A6A3FF");
        assert_eq!(look.mode, "#DB2777");
        assert_eq!(look.colours().text, suhina_dj::look::Rgba::hex("#ECE8E1").unwrap());
    }

    /// Every YOBUKO look reads: its text on its ground and on its panels. With
    /// `YOBUKO_LOOKS=<folder>` set, each look is also written there (for screenshots).
    #[test]
    fn every_yobuko_look_reads() {
        let themes = crate::settings::themes::Themes::default();
        let ids: Vec<String> = std::iter::once("yobuko".to_owned())
            .chain(theme::built_in_variants().into_iter().map(|(id, _)| id).filter(|id| id.starts_with("yobuko@")))
            .collect();
        assert!(ids.len() >= 10, "{ids:?}");
        let out = std::env::var_os("YOBUKO_LOOKS").map(std::path::PathBuf::from);
        for id in ids {
            let palette = themes.palette(&id);
            for (what, ground) in [("ground", palette.window_background), ("panels", palette.panel_background)] {
                let ratio = theme::contrast(palette.text_primary, ground);
                assert!(ratio >= 4.5, "{id}: text on {what} is {ratio:.1}:1");
            }
            if let Some(out) = &out {
                let look = look_of(&palette, &id, WorkspaceMode::Dj.color());
                let file = out.join(format!("{}.json", id.replace('@', "_")));
                suhina_dj::look::write(&file, &look).unwrap();
            }
        }
    }
}
