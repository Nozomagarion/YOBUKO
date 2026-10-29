// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::kit::waveform;

impl App {
    pub(super) fn show_dj_waveform(
        &mut self,
        ui: &mut Ui,
        id: u64,
        end: f64,
        busy: bool,
        cues: &[serde_json::Value],
        preview: &serde_json::Value,
    ) {
        ui.horizontal(|ui| {
            ui.label("Track waveform");
            if ui.add_enabled(!busy, egui::Button::new("Reload waveform")).clicked() {
                self.dj_command("dj.waveform.load", json!({"track":id}));
            }
        });
        let state = &self.dj.snapshot["waveform"];
        if state["track"].as_u64() == Some(id) {
            if state["loading"] == true {
                ui.label("Loading waveform…");
            }
            if let Some(error) = state["error"].as_str() {
                ui.colored_label(theme::accent(), format!("Waveform: {error}"));
            }
        }
        let Some(peaks) = u32::try_from(id).ok().and_then(|id| self.projects.dj_waveform(id)) else { return };
        let seconds = peaks.frames as f64 / f64::from(peaks.sample_rate.max(1));
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width().clamp(1.0, 4096.0), 96.0),
            egui::Sense::click_and_drag(),
        );
        let response =
            response.on_hover_text("Click or drag to choose a start position. Press Play from position to listen.");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "DJ track waveform"));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
        let count = rect.width().ceil() as usize;
        let columns = waveform::steady_columns(rect.left(), count, 0.0, seconds / f64::from(rect.width()), 1.0);
        let geometry = waveform::geometry(&peaks, rect, &columns, waveform::scale(ui.ctx()));
        waveform::paint(&painter, &geometry, waveform::Colors::from(theme::text_secondary()));
        let x = |time: f64| rect.left() + (time / seconds).clamp(0.0, 1.0) as f32 * rect.width();
        if let Some((grid, draft)) = self.dj_grid_for_waveform(id) {
            let color =
                if draft { egui::Color32::from_rgb(235, 185, 83) } else { egui::Color32::from_rgb(83, 170, 235) };
            let mut previous_label = f32::NEG_INFINITY;
            for line in grid.lines(seconds, (rect.width() / 12.0).max(1.0) as usize) {
                let at = x(line.seconds);
                let bar = line.index % 4 == 0;
                painter.line_segment(
                    [egui::pos2(at, rect.top()), egui::pos2(at, rect.bottom())],
                    egui::Stroke::new(1.0, color.gamma_multiply(if bar { 0.8 } else { 0.35 })),
                );
                if bar && at - previous_label >= 48.0 {
                    painter.text(
                        egui::pos2(at + 2.0, rect.bottom() - 2.0),
                        egui::Align2::LEFT_BOTTOM,
                        (line.index / 4 + 1).to_string(),
                        egui::FontId::proportional(10.0),
                        color,
                    );
                    previous_label = at;
                }
            }
        }
        for cue in cues {
            let Some(time) = cue["seconds"].as_f64().filter(|s| s.is_finite()) else { continue };
            let at = x(time);
            painter.line_segment(
                [egui::pos2(at, rect.top()), egui::pos2(at, rect.bottom())],
                egui::Stroke::new(1.0, theme::accent()),
            );
            painter.text(
                egui::pos2(at + 3.0, rect.top() + 3.0),
                egui::Align2::LEFT_TOP,
                cue["name"].as_str().unwrap_or("Cue"),
                egui::FontId::proportional(11.0),
                theme::accent(),
            );
        }
        let start = x(self.dj.seek);
        painter.line_segment(
            [egui::pos2(start, rect.top()), egui::pos2(start, rect.bottom())],
            egui::Stroke::new(2.0, ui.visuals().text_color()),
        );
        if preview["track"].as_u64() == Some(id) && preview["playing"] == true {
            let at = x(preview["position"].as_f64().unwrap_or(0.0));
            painter.line_segment(
                [egui::pos2(at, rect.top()), egui::pos2(at, rect.bottom())],
                egui::Stroke::new(2.0, egui::Color32::LIGHT_GREEN),
            );
        }
        if (response.clicked() || response.dragged())
            && let Some(at) = response.interact_pointer_pos()
        {
            self.dj.seek = position_at(rect, at.x, seconds, end);
        }
        ui.horizontal(|ui| {
            ui.weak("0 s");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.weak(format!("{seconds:.3} s"));
            });
        });
        ui.weak("Click to choose start; Play to listen. White: next start · green: playback.");
    }
}
fn position_at(rect: egui::Rect, x: f32, seconds: f64, end: f64) -> f64 {
    // Catalog duration may differ by the decoder's accepted 0.05 s tolerance.
    // Even then the right edge must remain before the decoded file's end.
    let end = end.min((seconds - 0.001).max(0.0));
    (f64::from(((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0)) * seconds).clamp(0.0, end)
}

#[cfg(test)]
mod tests {
    #[test]
    fn waveform_positions_use_file_seconds_and_clamp_before_the_end() {
        let rect = egui::Rect::from_min_size(egui::pos2(20.0, 0.0), egui::vec2(400.0, 96.0));
        assert_eq!(super::position_at(rect, 220.0, 2.0, 1.999), 1.0);
        assert_eq!(super::position_at(rect, -20.0, 2.0, 1.999), 0.0);
        assert_eq!(super::position_at(rect, 600.0, 2.0, 1.999), 1.999);
        assert_eq!(super::position_at(rect, 600.0, 2.0, 2.05), 1.999);
    }
}
