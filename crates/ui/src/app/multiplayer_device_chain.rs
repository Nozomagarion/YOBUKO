// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Explicit revision-bound chain edits; presets, paths, arbitrary devices and instrument replacement stay closed.
use super::*;
use suhina_engine::model::DeviceKind;

#[derive(Default)]
pub(super) struct ChainDraft {
    pub(super) track: Option<u32>,
    base: Option<u64>,
}
impl App {
    pub(super) fn show_shared_device_chain(&mut self, ui: &mut Ui) {
        let Some(session) = self.multiplayer.bound.and_then(|id| self.projects.get(id)) else { return };
        let project = session.project();
        let tracks: Vec<_> = project.tracks.iter().filter(|t| t.channel.is_some()).collect();
        let revision = self.multiplayer.controls.as_ref().map_or(0, |c| c.revision);
        let draft = &mut self.multiplayer.devices.chain;
        if draft.track.is_none() {
            draft.track = tracks.first().map(|t| t.id.0);
        }
        let old = draft.track;
        egui::ComboBox::from_label("Device track")
            .width(300.0)
            .selected_text(
                tracks.iter().find(|t| Some(t.id.0) == draft.track).map_or("Choose track", |t| t.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for t in &tracks {
                    ui.selectable_value(&mut draft.track, Some(t.id.0), &t.name);
                }
            });
        if old != draft.track {
            draft.base = Some(revision);
        }
        let base = *draft.base.get_or_insert(revision);
        let Some(track) = tracks.iter().find(|t| Some(t.id.0) == draft.track) else { return };
        let Some(channel) = track.channel.and_then(|id| project.channel(id)) else { return };
        let count = project.channels.iter().map(|c| c.devices.len()).sum::<usize>();
        ui.label(format!(
            "Device chain: {} / 16 on this track · {count} / 128 in the shared project",
            channel.devices.len()
        ));
        ui.label(if base != revision {
            "The shared project changed. Refresh this chain draft before adding."
        } else {
            "This chain draft matches the confirmed revision."
        });
        if ui.add_enabled(base != revision, egui::Button::new("Refresh chain draft")).clicked() {
            draft.base = Some(revision);
        }
        let ready =
            draft.base == Some(revision) && !self.multiplayer.submitted && channel.devices.len() < 16 && count < 128;
        let fm = track.content.notes
            && !channel.devices.iter().any(|d| matches!(&d.kind,DeviceKind::Builtin(id) if id=="fm-synth"));
        let kind = ui.horizontal(|ui| {
            let fm = ui.add_enabled(ready && fm,egui::Button::new("Add FM Synth"))
                .on_hover_text("Adds the default instrument to a MIDI track without an instrument. Existing instruments are never replaced.").clicked();
            let utility = ui.add_enabled(ready,egui::Button::new("Add Utility")).clicked();
            if fm {Some("fm-synth")} else if utility {Some("utility")} else {None}
        }).inner;
        let track_id = track.id.0;
        ui.weak("FM Synth goes before effects on a MIDI track without an instrument. Utility is appended. Removing FM Synth keeps the notes and restores the track's basic sound.");
        ui.separator();
        if let Some(kind) = kind {
            let edit = suhina_multiplayer::WireEdit::DeviceAdd { track: track_id, kind: kind.into() };
            match serde_json::to_string(&edit) {
                Ok(edit) => self.multiplayer_command("multiplayer.edit", json!({"base_revision":revision,"edit":edit})),
                Err(error) => self.multiplayer.error = Some(error.to_string()),
            }
        }
    }
}
