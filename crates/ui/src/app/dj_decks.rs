// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
impl App {
    pub(in crate::app) fn stop_dj_decks(&mut self) {
        if !self.dj.decks_owned {
            return;
        }
        match self.projects.execute("dj.deck.stopAll", &json!({})) {
            Ok(_) => {
                self.dj.decks_owned = false;
                self.dj.next_poll = None;
            }
            Err(e) => self.status = Status::Error(e.to_string()),
        }
    }
    pub(super) fn performance_command(&mut self, command: &str, params: serde_json::Value) {
        match self.projects.execute(command, &params) {
            Ok(_) => {
                self.dj.decks_owned = true;
                self.dj.next_poll = None;
                if command == "dj.deck.load" {
                    self.dj.snapshot["busy"] = json!(true);
                }
                self.dj.snapshot["error"] = serde_json::Value::Null;
            }
            Err(e) => self.dj.snapshot["error"] = json!(e.to_string()),
        }
    }
    pub(super) fn show_dj_decks(&mut self, ui: &mut Ui) {
        let perf = self.dj.snapshot["performance"].clone();
        ui.heading("Performance · decks A / B");
        ui.weak("Original speed · −6 dB trim · no sync or tempo changes. Loading does not play audio.");
        ui.columns(2, |cols| {
            for (i, ui) in cols.iter_mut().enumerate() {
                let name = if i == 0 { "A" } else { "B" };
                let d = &perf["decks"][i];
                ui.strong(format!("Deck {name} · {}", d["title"].as_str().unwrap_or("No track loaded")));
                let loaded = d["track"].is_number();
                let loading = d["loading"] == true;
                let seconds = d["seconds"].as_f64().unwrap_or(0.001);
                let pos = d["position"].as_f64().unwrap_or(0.0);
                ui.label(format!(
                    "{} · {pos:.3} / {seconds:.3} s",
                    if loading {
                        "Loading"
                    } else if d["playing"] == true {
                        "Playing"
                    } else {
                        "Stopped / paused"
                    }
                ));
                if let Some(error) = d["error"].as_str() {
                    ui.colored_label(theme::accent(), error);
                }
                ui.horizontal(|ui| {
                    for (button, command, enabled) in [
                        ("Play", "dj.deck.play", loaded && !loading),
                        ("Pause", "dj.deck.pause", loaded),
                        ("Stop", "dj.deck.stop", true),
                        ("Unload", "dj.deck.clear", loaded || loading),
                    ] {
                        if ui.add_enabled(enabled, egui::Button::new(format!("{button} deck {name}"))).clicked() {
                            self.performance_command(command, json!({"deck":name}));
                        }
                    }
                });
                ui.horizontal(|ui| {
                    let label = ui.label(format!("Deck {name} seek seconds"));
                    ui.add(
                        egui::DragValue::new(&mut self.dj.deck_seek[i])
                            .range(0.0..=(seconds - 0.001).max(0.0))
                            .speed(0.01)
                            .max_decimals(3),
                    )
                    .labelled_by(label.id);
                    if ui.add_enabled(loaded, egui::Button::new(format!("Seek deck {name}"))).clicked() {
                        self.performance_command("dj.deck.seek", json!({"deck":name,"from":self.dj.deck_seek[i]}));
                    }
                });
            }
        });
        let mut x = perf["crossfade"].as_f64().unwrap_or(0.5);
        let mut a = perf["levelA"].as_f64().unwrap_or(1.0);
        let mut b = perf["levelB"].as_f64().unwrap_or(1.0);
        let mut ca = perf["cueA"].as_bool().unwrap_or(false);
        let mut cb = perf["cueB"].as_bool().unwrap_or(false);
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            let label = ui.label("Crossfader A / B");
            changed |= ui.add(egui::Slider::new(&mut x, 0.0..=1.0)).labelled_by(label.id).changed();
            for (name, value) in [("Deck A level", &mut a), ("Deck B level", &mut b)] {
                let label = ui.label(name);
                changed |= ui.add(egui::Slider::new(value, 0.0..=1.0)).labelled_by(label.id).changed();
            }
            ui.add_enabled_ui(perf["separate_cue"] == true, |ui| {
                changed |= ui.checkbox(&mut ca, "Cue deck A").changed();
                changed |= ui.checkbox(&mut cb, "Cue deck B").changed();
            });
        });
        if changed {
            self.performance_command("dj.mixer.set", json!({"crossfade":x,"levelA":a,"levelB":b,"cueA":ca,"cueB":cb}));
        }
        if perf["separate_cue"] != true {
            ui.weak("Headphone deck cues require a separate cue output pair in Audio settings; hidden decks are not added to main.");
        }
        ui.separator();
    }
}
