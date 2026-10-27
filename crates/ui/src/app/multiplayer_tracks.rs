// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Revision-bound shared track creation; native track.add and file/device options stay local.
use super::*;

#[derive(Default)]
pub(super) struct TrackEditor {
    pub(super) name: String,
    pub(super) midi: bool,
    base: Option<u64>,
}
impl App {
    pub(super) fn show_shared_tracks(&mut self, ui: &mut Ui) {
        let Some(session) = self.multiplayer.bound.and_then(|id| self.projects.get(id)) else { return };
        let count = session.project().tracks.len();
        let revision = self.multiplayer.controls.as_ref().map_or(0, |c| c.revision);
        let editor = &mut self.multiplayer.tracks;
        let base = *editor.base.get_or_insert(revision);
        ui.label(format!("Shared tracks: {count} / 64 (including returns and groups)."));
        ui.weak("Both participants can add tracks. Audio starts empty; MIDI includes FM Synth. Local hardware inputs stay off.");
        if base != revision {
            ui.label("The shared project changed. Refresh this draft before adding another track.");
            if ui.button("Refresh track draft").clicked() {
                editor.base = Some(revision);
                editor.name.clear();
            }
        }
        ui.horizontal(|ui| {
            let label = ui.label("New track name");
            ui.add(egui::TextEdit::singleline(&mut editor.name).desired_width(220.0).char_limit(256))
                .labelled_by(label.id);
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut editor.midi, false, "Audio track");
            ui.radio_value(&mut editor.midi, true, "MIDI track · FM Synth");
        });
        let valid =
            !editor.name.trim().is_empty() && editor.name.len() <= 256 && !editor.name.chars().any(char::is_control);
        if editor.name.len() > 256 {
            ui.label("Choose a shorter track name.");
        }
        let ready = base == revision && count < 64 && valid && !self.multiplayer.submitted;
        if count >= 64 {
            ui.label("The 64-track limit is reached for this session.");
        }
        if ui.add_enabled(ready, egui::Button::new("Add shared track")).clicked() {
            let edit = suhina_multiplayer::WireEdit::TrackCreate {
                kind: if editor.midi { "midi" } else { "audio" }.into(),
                name: editor.name.clone(),
            };
            match serde_json::to_string(&edit) {
                Ok(edit) => self.multiplayer_command("multiplayer.edit", json!({"base_revision":base,"edit":edit})),
                Err(error) => self.multiplayer.error = Some(error.to_string()),
            }
        }
        ui.weak("Use Clips to create a clip on the new track, MIDI notes to compose, or Publish take to place a finished sound on a new audio track.");
    }
}
