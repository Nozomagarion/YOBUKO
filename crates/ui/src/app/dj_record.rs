// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Recording the mix: the REC button in the DJ space's header starts and stops the session's
//! `dj.record.*`, shows how long it has run, and says where the file went.

use super::*;

/// The recording's colour.
const REC_COLOR: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);

/// Where recordings go: a folder in the Music folder, else in the home folder.
pub(super) fn recordings_folder() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)?;
    let music = home.join("Music");
    Some(if music.is_dir() { music } else { home }.join("YOBUKO Recordings"))
}

/// The button's text: the time recorded, while recording.
pub(super) fn rec_text(on: bool, seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    if on { format!("REC {}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { "REC".to_owned() }
}

impl Decks {
    /// What the session says of the mix's recording.
    pub(super) fn recording(&self) -> &Value {
        &self.snapshot["performance"]["recording"]
    }
}

impl App {
    /// Starts recording the mix into `folder`, or stops it.
    pub(super) fn toggle_recording(&mut self, folder: Option<PathBuf>) {
        if self.dj.decks.recording()["on"] == true {
            if self.deck_command("dj.record.stop", json!({})) {
                self.dj.decks.snapshot["performance"]["recording"]["on"] = json!(false);
            }
        } else if let Some(folder) = folder {
            if self.deck_command("dj.record.start", json!({"folder": folder})) {
                self.dj.decks.snapshot["performance"]["recording"]["on"] = json!(true);
                self.dj.decks.snapshot["performance"]["recording"]["seconds"] = json!(0.0);
            }
        } else {
            self.dj.decks.error = Some("no home folder to save the recording in".into());
        }
    }

    /// The REC button: red and counting while the mix is recorded.
    pub(super) fn record_button(&mut self, ui: &mut Ui) {
        let rec = self.dj.decks.recording();
        let on = rec["on"] == true;
        let seconds = rec["seconds"].as_f64().unwrap_or(0.0);
        let last = rec["last"].as_str().map(str::to_owned);
        let error = rec["error"].as_str().map(str::to_owned);
        let text = rec_text(on, seconds);
        let ink = if on { Color32::WHITE } else { REC_COLOR };
        // Room on the left for the dot the button paints (the fonts have no round dot glyph).
        let mut job = egui::text::LayoutJob::default();
        let font = egui::TextStyle::Button.resolve(ui.style());
        job.append(&text, 14.0, egui::TextFormat::simple(font, ink));
        let mut button = egui::Button::new(job).corner_radius(BUTTON_RADIUS);
        if on {
            button = button.fill(REC_COLOR);
        }
        let response = ui.add(button);
        ui.painter().circle_filled(response.rect.left_center() + Vec2::new(14.0, 0.0), 4.5, ink);
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, "Record the mix"));
        let hover = match (on, &error, &last) {
            (true, ..) => "Recording what the master plays · Click: stop and save".to_owned(),
            (false, Some(e), _) => format!("The last recording had a problem: {e}"),
            (false, None, Some(path)) => format!("Click: record the mix · Last one saved in {path}"),
            (false, None, None) => "Click: record the mix to a WAV file in Music › YOBUKO Recordings".to_owned(),
        };
        if response.on_hover_text(hover).clicked() {
            self.toggle_recording(recordings_folder());
        }
    }
}
