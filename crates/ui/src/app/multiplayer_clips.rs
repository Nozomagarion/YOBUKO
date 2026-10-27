// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Stable-ID arrangement edits sent through the same authoritative service as MIDI notes.
use super::*;
use suhina_multiplayer::WireEdit;

#[derive(Clone)]
struct ClipRow {
    id: u32,
    track: u32,
    name: String,
    start: f64,
    length: f64,
    enabled: bool,
}
#[derive(Default)]
pub(super) struct ClipEditor {
    cached: Option<u64>,
    rows: Vec<ClipRow>,
    draft: Option<ClipRow>,
    base: Option<u64>,
    page: usize,
    create_track: Option<u32>,
    create_start: f64,
    create_length: f64,
}
impl App {
    pub(super) fn show_shared_clips(&mut self, ui: &mut Ui) {
        let Some(id) = self.multiplayer.bound else { return };
        let Some(session) = self.projects.get(id) else { return };
        let project = session.project();
        let revision = self.multiplayer.controls.as_ref().map_or(0, |c| c.revision);
        let tracks: Vec<_> = project
            .tracks
            .iter()
            .filter(|t| t.content.notes || t.content.audio)
            .map(|t| (t.id.0, t.name.clone()))
            .collect();
        if tracks.is_empty() {
            ui.weak("Add a shared track in Tracks, then create its clip here.");
            return;
        }
        let editor = &mut self.multiplayer.clips;
        if editor.base.is_none() {
            editor.base = Some(revision);
            editor.create_track = tracks.first().map(|t| t.0);
            editor.create_length = 4.0;
        }
        if editor.cached != Some(revision) {
            editor.rows = project
                .arrangement
                .lanes
                .iter()
                .filter_map(|lane| lane.track.map(|track| (lane, track)))
                .flat_map(|(lane, track)| {
                    lane.clips.iter().map(move |clip| ClipRow {
                        id: clip.id.0,
                        track: track.0,
                        name: project.content(clip.content).map_or("MIDI", |c| c.name.as_str()).to_owned(),
                        start: clip.start,
                        length: clip.length,
                        enabled: clip.enabled,
                    })
                })
                .collect();
            editor.cached = Some(revision);
        }
        ui.weak("Arrangement clips · moves and trims keep neighbouring clips. Select a clip to edit it.");
        let pages = editor.rows.len().div_ceil(24).max(1);
        editor.page = editor.page.min(pages - 1);
        ui.horizontal(|ui| {
            if ui.add_enabled(editor.page > 0, egui::Button::new("Previous clips")).clicked() {
                editor.page -= 1;
            }
            ui.label(format!("{} clips · page {} / {pages}", editor.rows.len(), editor.page + 1));
            if ui.add_enabled(editor.page + 1 < pages, egui::Button::new("Next clips")).clicked() {
                editor.page += 1;
            }
        });
        egui::ScrollArea::vertical().id_salt("shared-clip-list").max_height(180.0).show(ui, |ui| {
            egui::Grid::new("shared-clips").striped(true).spacing(Vec2::new(12.0, 8.0)).show(ui, |ui| {
                for title in ["Clip", "Name", "Track", "Start", "Length", "Enabled"] {
                    ui.strong(title);
                }
                ui.end_row();
                for row in editor.rows.iter().skip(editor.page * 24).take(24) {
                    if ui
                        .selectable_label(
                            editor.draft.as_ref().is_some_and(|d| d.id == row.id),
                            format!("Edit clip {}", row.id),
                        )
                        .clicked()
                    {
                        editor.draft = Some(row.clone());
                        editor.base = Some(revision);
                    }
                    ui.add_sized([180.0, 18.0], egui::Label::new(&row.name).truncate()).on_hover_text(&row.name);
                    let track_name = tracks.iter().find(|t| t.0 == row.track).map_or("Track", |t| t.1.as_str());
                    ui.add_sized([150.0, 18.0], egui::Label::new(track_name).truncate()).on_hover_text(track_name);
                    ui.label(format!("{:.3}", row.start));
                    ui.label(format!("{:.3}", row.length));
                    ui.label(if row.enabled { "Yes" } else { "No" });
                    ui.end_row();
                }
            });
        });
        let stale = editor.base != Some(revision);
        if stale {
            ui.colored_label(theme::recording(), "New shared changes arrived. Refresh this draft before applying it.");
            if ui.button("Refresh clip draft").clicked() {
                editor.draft = None;
                editor.base = Some(revision);
            }
        }
        let mut action = None;
        if let Some(draft) = &mut editor.draft {
            ui.separator();
            ui.label(format!("Selected clip #{} · {}", draft.id, draft.name));
            ui.horizontal_wrapped(|ui| {
                ui.label("Start beat");
                ui.add(egui::DragValue::new(&mut draft.start).range(0.0..=120_000.0).speed(0.25));
                ui.label("Length");
                ui.add(egui::DragValue::new(&mut draft.length).range(0.001..=120_000.0).speed(0.25));
                egui::ComboBox::from_id_salt("shared-clip-track")
                    .width(180.0)
                    .wrap_mode(egui::TextWrapMode::Truncate)
                    .selected_text(tracks.iter().find(|t| t.0 == draft.track).map_or("Track", |t| t.1.as_str()))
                    .show_ui(ui, |ui| {
                        for (id, name) in &tracks {
                            ui.selectable_value(&mut draft.track, *id, name);
                        }
                    });
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(!stale, egui::Button::new("Move clip")).clicked() {
                    action = Some(WireEdit::ClipMove { clip: draft.id, start: draft.start, track: draft.track });
                }
                if ui.add_enabled(!stale, egui::Button::new("Trim clip edges")).clicked() {
                    action = Some(WireEdit::ClipTrim {
                        clip: draft.id,
                        start: draft.start,
                        end: draft.start + draft.length,
                    });
                }
                if ui.add_enabled(!stale, egui::Button::new("Duplicate clip")).clicked() {
                    action = Some(WireEdit::ClipDuplicate { clip: draft.id });
                }
                if ui
                    .add_enabled(!stale, egui::Button::new(if draft.enabled { "Silence clip" } else { "Enable clip" }))
                    .clicked()
                {
                    action = Some(WireEdit::ClipEnabled { clip: draft.id, enabled: !draft.enabled });
                }
                if ui.add_enabled(!stale, egui::Button::new("Delete clip")).clicked() {
                    action = Some(WireEdit::ClipDelete { clip: draft.id });
                }
            });
            ui.weak("Move changes the clip's position and track. Trim changes its edges and playback offset; the track selector only applies to Move.");
        }
        ui.separator();
        ui.label("New clip");
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("shared-new-clip-track")
                .width(180.0)
                .wrap_mode(egui::TextWrapMode::Truncate)
                .selected_text(
                    tracks.iter().find(|t| Some(t.0) == editor.create_track).map_or("Track", |t| t.1.as_str()),
                )
                .show_ui(ui, |ui| {
                    for (id, name) in &tracks {
                        ui.selectable_value(&mut editor.create_track, Some(*id), name);
                    }
                });
            ui.label("Start beat");
            ui.add(egui::DragValue::new(&mut editor.create_start).range(0.0..=120_000.0).speed(0.25));
            ui.label("Length");
            ui.add(egui::DragValue::new(&mut editor.create_length).range(0.001..=120_000.0).speed(0.25));
            if ui.add_enabled(!stale, egui::Button::new("Create clip")).clicked()
                && let Some(track) = editor.create_track
            {
                action = Some(WireEdit::ClipCreate { track, start: editor.create_start, length: editor.create_length });
            }
        });
        ui.weak(format!(
            "Confirmed revision {revision}. New clips start empty. Use MIDI notes on a MIDI track; new audio clips have no sound file yet."
        ));
        if let Some(action) = action
            && let Ok(edit) = serde_json::to_string(&action)
        {
            let base = editor.base.unwrap_or(revision);
            self.multiplayer_command("multiplayer.edit", json!({"base_revision":base,"edit":edit}));
        }
    }
}
