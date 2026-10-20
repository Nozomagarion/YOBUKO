// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The same tiny musical scene in each palette. No theme installation or project loading.

use egui::{Painter, Rect, Stroke, pos2, vec2};

use super::theme::Palette;

/// Paint a miniature browser, timeline, MIDI/audio clips and controls from explicit colours.
pub fn paint(painter: &Painter, rect: Rect, palette: &Palette) {
    if !painter.clip_rect().intersects(rect) || !rect.is_finite() || rect.width() < 30.0 || rect.height() < 20.0 {
        return;
    }
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let cell = |x: f32, y: f32, w: f32, h: f32| {
        Rect::from_min_size(
            pos2(rect.left() + rect.width() * x, rect.top() + rect.height() * y),
            vec2(rect.width() * w, rect.height() * h),
        )
    };
    painter.rect_filled(rect, 2.0, palette.window_background);
    painter.rect_filled(cell(0.0, 0.0, 1.0, 0.15), 0.0, palette.panel_background);
    painter.rect_filled(cell(0.0, 0.15, 0.2, 0.85), 0.0, palette.panel_background);
    for row in 0..5 {
        painter.rect_filled(cell(0.025, 0.22 + row as f32 * 0.11, 0.13, 0.018), 0.0, palette.text_secondary);
    }
    for button in 0..4 {
        painter.rect_filled(
            cell(0.025 + button as f32 * 0.06, 0.04, 0.04, 0.065),
            1.0,
            if button == 0 { palette.accent } else { palette.control_background },
        );
    }
    for bar in 0..8 {
        let grid = cell(0.23 + bar as f32 * 0.09, 0.2, 0.0, 0.75);
        painter.vline(grid.left(), grid.y_range(), Stroke::new(0.6, palette.grid_major));
    }
    for (row, colour) in [palette.track_color_1, palette.track_color_3, palette.track_color_4].into_iter().enumerate() {
        let top = 0.24 + row as f32 * 0.23;
        let clip = cell(0.25 + row as f32 * 0.075, top, 0.55 - row as f32 * 0.055, 0.18);
        painter.rect_filled(clip, 2.0, colour);
        let ink = if super::theme::contrast(colour, palette.text_primary)
            > super::theme::contrast(colour, palette.window_background)
        {
            palette.text_primary
        } else {
            palette.window_background
        };
        if row == 1 {
            for sample in 0..24 {
                let x = clip.left() + clip.width() * (sample as f32 + 0.5) / 24.0;
                let amplitude = clip.height() * (0.1 + ((sample * 7 % 13) as f32 / 13.0) * 0.3);
                painter.vline(x, (clip.center().y - amplitude)..=(clip.center().y + amplitude), Stroke::new(0.8, ink));
            }
        } else {
            for note in 0..6 {
                let y = clip.top() + clip.height() * (0.25 + (note % 3) as f32 * 0.22);
                let x = clip.left() + clip.width() * (0.06 + note as f32 * 0.145);
                painter.line_segment([pos2(x, y), pos2(x + clip.width() * 0.09, y)], Stroke::new(1.1, ink));
            }
        }
        painter.rect_filled(cell(0.93, top + 0.03, 0.025, 0.12), 0.0, palette.meter_normal);
    }
    let playhead = cell(0.64, 0.18, 0.0, 0.78);
    painter.vline(playhead.left(), playhead.y_range(), Stroke::new(1.0, palette.accent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_uses_its_explicit_palette_without_installing_it() {
        let ctx = egui::Context::default();
        let current = super::super::theme::palette();
        let preview = Palette { window_background: egui::Color32::from_rgb(24, 60, 90), ..current };
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(240.0, 92.0));
        let mut output = ctx.run_ui(Default::default(), |ui| paint(ui.painter(), rect, &preview));
        assert_eq!(super::super::theme::palette(), current);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Rect(shape) if shape.fill == preview.window_background)));
        assert!(output.shapes.iter().all(|shape| shape.clip_rect == rect));
        output.textures_delta.clear();
    }
}
