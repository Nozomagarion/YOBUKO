// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use suhina_session::DjBeatGrid;

pub(in crate::app) struct GridDraft {
    pub revision: u64,
    pub track: u64,
    pub bpm: f64,
    pub first: f64,
    pub preview: bool,
}
fn saved(snapshot: &serde_json::Value, id: u64) -> Option<DjBeatGrid> {
    snapshot["grids"]
        .as_array()?
        .iter()
        .find(|g| g["track"].as_u64() == Some(id))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}
impl App {
    pub(super) fn dj_grid_for_waveform(&self, id: u64) -> Option<(DjBeatGrid, bool)> {
        if let Some(d) = self
            .dj
            .grid_draft
            .as_ref()
            .filter(|d| d.track == id && d.preview && Some(d.revision) == self.dj.snapshot["revision"].as_u64())
        {
            return Some((DjBeatGrid { track: u32::try_from(id).ok()?, bpm: d.bpm, first_beat: d.first }, true));
        }
        saved(&self.dj.snapshot, id).map(|g| (g, false))
    }
    fn queue_dj_grid(&mut self, command: &str, params: serde_json::Value) {
        match self.projects.execute(command, &params) {
            Ok(_) => {
                self.dj.grid_queued = true;
                self.dj.grid_delete = None;
                self.dj.snapshot["busy"] = json!(true);
                self.dj.snapshot["error"] = serde_json::Value::Null;
                self.dj.next_poll = None;
            }
            Err(e) => self.dj.snapshot["error"] = json!(e.to_string()),
        }
    }
    pub(super) fn show_dj_grid(&mut self, ui: &mut Ui, track: &serde_json::Value, revision: u64, busy: bool) {
        let Some(id) = track["id"].as_u64() else { return };
        let original = saved(&self.dj.snapshot, id);
        if self.dj.grid_draft.is_none() {
            self.dj.grid_draft = Some(GridDraft {
                track: id,
                revision,
                bpm: original.as_ref().map(|g| g.bpm).or(track["bpm"].as_f64()).unwrap_or(120.0),
                first: original.as_ref().map_or(0.0, |g| g.first_beat),
                preview: false,
            });
        }
        if self.dj.grid_delete.is_some_and(|(t, r)| t != id || r != revision) {
            self.dj.grid_delete = None;
        }
        let Some(draft) = self.dj.grid_draft.as_mut() else { return };
        let stale = draft.revision != revision || draft.track != id;
        let end = (track["seconds"].as_f64().unwrap_or(0.001) - 0.001).max(0.0);
        ui.label(format!(
            "Beat grid · constant tempo · 4 beats/bar · analysed: {} · {}",
            track["bpm"].as_f64().map_or_else(|| "unknown".into(), |b| format!("{b:.2} BPM")),
            original.as_ref().map_or_else(
                || "no saved grid".into(),
                |g| format!("saved {:.2} BPM, first beat {:.3} s", g.bpm, g.first_beat)
            )
        ));
        let mut refresh = false;
        let mut save = false;
        let mut clear = false;
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                let label = ui.label("Grid BPM");
                if ui
                    .add(egui::DragValue::new(&mut draft.bpm).range(20.0..=400.0).speed(0.01).max_decimals(3))
                    .labelled_by(label.id)
                    .changed()
                {
                    draft.preview = true;
                }
                let label = ui.label("First beat seconds");
                if ui
                    .add(egui::DragValue::new(&mut draft.first).range(0.0..=end).speed(0.001).max_decimals(3))
                    .labelled_by(label.id)
                    .changed()
                {
                    draft.preview = true;
                }
                if ui.button("Use start as first beat").clicked() {
                    draft.first = self.dj.seek.clamp(0.0, end);
                    draft.preview = true;
                }
                if ui.add_enabled(draft.bpm >= 40.0, egui::Button::new("Half grid tempo")).clicked() {
                    draft.bpm /= 2.0;
                    draft.preview = true;
                }
                if ui.add_enabled(draft.bpm <= 200.0, egui::Button::new("Double grid tempo")).clicked() {
                    draft.bpm *= 2.0;
                    draft.preview = true;
                }
            });
            if stale {
                ui.horizontal(|ui| {
                    ui.label("Library changed; refresh this grid draft before saving.");
                    if ui.button("Refresh grid draft").clicked() {
                        refresh = true;
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(!stale, egui::Button::new("Preview beat grid")).clicked() {
                    draft.preview = true;
                }
                if ui.add_enabled(!stale && track["bpm"].is_number(), egui::Button::new("Use analysed tempo")).clicked()
                {
                    draft.bpm = track["bpm"].as_f64().unwrap_or(120.0);
                    draft.preview = true;
                }
                if ui.add_enabled(!stale, egui::Button::new("Save beat grid")).clicked() {
                    save = true;
                }
                if ui.add_enabled(!stale && original.is_some(), egui::Button::new("Delete beat grid…")).clicked() {
                    self.dj.grid_delete = Some((id, revision));
                }
                if draft.preview && !stale {
                    ui.weak("Draft grid displayed · not saved");
                }
            });
            if self.dj.grid_delete.is_some() {
                ui.horizontal(|ui| {
                    ui.label("Delete the saved grid? Audio and cue points stay unchanged.");
                    if ui.button("Confirm delete grid").clicked() {
                        clear = true;
                    }
                    if ui.button("Cancel grid deletion").clicked() {
                        self.dj.grid_delete = None;
                    }
                });
            }
        });
        let params = json!({"track":id,"revision":draft.revision,"bpm":draft.bpm,"firstBeat":draft.first});
        if refresh {
            self.dj.grid_draft = None;
            self.dj.grid_delete = None;
        } else if save {
            self.queue_dj_grid("dj.grid.set", params);
        } else if clear {
            self.queue_dj_grid("dj.grid.clear", json!({"track":id,"revision":revision}));
        }
    }
}
