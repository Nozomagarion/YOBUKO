// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The microphone on the DJ master: TALK puts it over the music, which ducks under it; the
//! input it reads, its level and how far the music drops are set beside it (`dj.mic.set`).

use super::tools::small_button;
use super::*;

/// The microphone's colour.
const MIC_COLOR: Color32 = Color32::from_rgb(0xA7, 0x8B, 0xFA);
/// The inputs the IN button steps through.
const INPUTS: u64 = 8;

impl Decks {
    /// What the session says of the microphone.
    pub(super) fn mic(&self) -> &Value {
        &self.snapshot["performance"]["mic"]
    }
}

impl App {
    /// Sends a change to the microphone, and shows it before the next read.
    pub(super) fn set_mic(&mut self, change: Value) {
        if self.deck_command("dj.mic.set", change.clone())
            && let (Some(mic), Some(change)) =
                (self.dj.decks.snapshot["performance"]["mic"].as_object_mut(), change.as_object())
        {
            for (key, value) in change {
                mic.insert(key.clone(), value.clone());
            }
        }
    }

    /// The microphone's row, under the crossfader.
    pub(super) fn mic_row(&mut self, ui: &mut Ui) {
        let mic = self.dj.decks.mic().clone();
        let on = mic["on"] == true;
        let channel = mic["channel"].as_u64().unwrap_or(1);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(egui::RichText::new("MIC").small().color(theme::text_hint()));
            if small_button(ui, "Talk on the microphone", "TALK", on, MIC_COLOR, true)
                .on_hover_text("The microphone on the master, over the music, which ducks under it")
                .clicked()
            {
                self.set_mic(json!({"on": !on}));
            }
            if small_button(ui, "Microphone input", &format!("IN {channel}"), false, MIC_COLOR, true)
                .on_hover_text("The audio input the microphone is on: click for the next")
                .clicked()
            {
                self.set_mic(json!({"channel": channel % INPUTS + 1}));
            }
            ui.spacing_mut().slider_width = 44.0;
            for (key, label, text, rest) in
                [("gain", "Microphone level", "LEVEL", 1.0), ("duck", "Music ducking", "DUCK", 0.5)]
            {
                ui.label(egui::RichText::new(text).small().color(theme::text_hint()));
                let mut value = mic[key].as_f64().unwrap_or(rest) as f32;
                let response = ui.add(egui::Slider::new(&mut value, 0.0..=1.0).show_value(false).trailing_fill(true));
                response.widget_info(|| egui::WidgetInfo::slider(true, f64::from(value), label));
                if response.on_hover_text(label).changed() {
                    let mut change = json!({});
                    change[key] = json!(f64::from(value));
                    self.set_mic(change);
                }
            }
        });
    }
}
