// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Revision-bound single-note editor; all writes go through the authoritative LAN service.
use super::*;
use suhina_multiplayer::WireEdit;

#[derive(Clone)]
struct NoteRow {
    index: usize,
    time: f64,
    length: f64,
    key: u8,
    velocity: f64,
    muted: bool,
}
impl NoteRow {
    fn read(value: &serde_json::Value) -> Option<Self> {
        Some(Self {
            index: usize::try_from(value["index"].as_u64()?).ok()?,
            time: value["time"].as_f64()?,
            length: value["length"].as_f64()?,
            key: u8::try_from(value["key"].as_u64()?).ok()?,
            velocity: value["velocity"].as_f64()?,
            muted: value["muted"].as_bool()?,
        })
    }
}
impl Default for NoteRow {
    fn default() -> Self {
        Self { index: 0, time: 0.0, length: 1.0, key: 60, velocity: 100.0, muted: false }
    }
}
#[derive(Default)]
pub(super) struct NoteEditor {
    clip: Option<u32>,
    cached: Option<(u32, u64)>,
    rows: Vec<NoteRow>,
    draft: NoteRow,
    selected: bool,
    base: u64,
    page: usize,
}
impl App {
    pub(super) fn show_shared_notes(&mut self, ui: &mut Ui) {
        let Some(id) = self.multiplayer.bound else { return };
        let Some(session) = self.projects.get(id) else { return };
        let project = session.project();
        let mut clips = Vec::new();
        let label = |track, clip: &suhina_engine::model::Clip, place: String| {
            let name = project.tracks.iter().find(|t| t.id == track).map_or("Track", |t| t.name.as_str());
            let content = project.content(clip.content).map_or("MIDI", |c| c.name.as_str());
            (clip.id.0, format!("{name} · {place} · {content} (#{} )", clip.id.0))
        };
        for lane in &project.arrangement.lanes {
            if let Some(track) = lane.track {
                for (i, clip) in lane.clips.iter().enumerate() {
                    clips.push(label(track, clip, format!("arrangement {}", i + 1)));
                }
            }
        }
        for column in &project.launcher.columns {
            for (i, slot) in column.slots.iter().enumerate() {
                if let Some(clip) = &slot.clip {
                    clips.push(label(column.track, clip, format!("scene {}", i + 1)));
                }
            }
        }
        if clips.is_empty() {
            ui.weak("This project has no MIDI clips to edit. Create clips in Studio before sharing it.");
            return;
        }
        let revision = self.multiplayer.controls.as_ref().map_or(0, |c| c.revision);
        let editor = &mut self.multiplayer.notes;
        if editor.clip.is_none() {
            editor.clip = clips.first().map(|c| c.0);
            editor.base = revision;
        }
        if editor.clip.is_some_and(|id| !clips.iter().any(|clip| clip.0 == id)) {
            ui.weak("The selected MIDI clip was deleted. Its note draft will not be applied to another clip.");
            if ui.button("Choose another MIDI clip").clicked() {
                *editor = Default::default();
            }
            return;
        }
        let before = editor.clip;
        egui::ComboBox::from_id_salt("shared-midi-clip")
            .width(420.0)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(clips.iter().find(|c| Some(c.0) == editor.clip).map_or("Choose clip", |c| c.1.as_str()))
            .show_ui(ui, |ui| {
                for (id, label) in &clips {
                    ui.selectable_value(&mut editor.clip, Some(*id), label);
                }
            });
        let Some(clip) = editor.clip else { return };
        if before != editor.clip {
            editor.selected = false;
            editor.draft = Default::default();
            editor.base = revision;
            editor.page = 0;
        }
        if editor.cached != Some((clip, revision)) {
            let result = self
                .projects
                .execute_in(Some(id), "note.list", &json!({"clip":clip}))
                .map_err(|e| e.to_string())
                .and_then(|value| {
                    value
                        .as_array()
                        .and_then(|rows| rows.iter().map(NoteRow::read).collect::<Option<Vec<_>>>())
                        .ok_or_else(|| "invalid native note list".to_owned())
                });
            match result {
                Ok(rows) => {
                    editor.rows = rows;
                    editor.cached = Some((clip, revision));
                }
                Err(error) => {
                    ui.colored_label(theme::recording(), error);
                    return;
                }
            }
        }
        ui.weak("Select a note, edit its values, then apply. Each computer keeps its own playback position.");
        let pages = editor.rows.len().div_ceil(32).max(1);
        editor.page = editor.page.min(pages - 1);
        ui.horizontal(|ui| {
            if ui.add_enabled(editor.page > 0, egui::Button::new("Previous notes")).clicked() {
                editor.page -= 1;
            }
            ui.label(format!("{} notes · page {} / {pages}", editor.rows.len(), editor.page + 1));
            if ui.add_enabled(editor.page + 1 < pages, egui::Button::new("Next notes")).clicked() {
                editor.page += 1;
            }
        });
        egui::ScrollArea::vertical().id_salt("shared-note-list").max_height(180.0).show(ui, |ui| {
            egui::Grid::new("shared-notes").striped(true).show(ui, |ui| {
                for title in ["Note", "Beat", "Length", "Key", "Velocity", "Muted"] {
                    ui.strong(title);
                }
                ui.end_row();
                for row in editor.rows.iter().skip(editor.page * 32).take(32) {
                    if ui
                        .selectable_label(
                            editor.selected && editor.draft.index == row.index,
                            format!("Edit note {}", row.index + 1),
                        )
                        .clicked()
                    {
                        editor.draft = row.clone();
                        editor.selected = true;
                        editor.base = revision;
                    }
                    ui.label(format!("{:.3}", row.time));
                    ui.label(format!("{:.3}", row.length));
                    ui.label(row.key.to_string());
                    ui.label(format!("{:.0}", row.velocity));
                    ui.label(if row.muted { "Yes" } else { "No" });
                    ui.end_row();
                }
            });
        });
        let stale = editor.base != revision;
        if stale {
            ui.colored_label(
                theme::recording(),
                "This draft is outdated. Refresh before editing another person's changes.",
            );
            if ui.button("Refresh note draft").clicked() {
                if editor.selected {
                    // Deleted/shifted indices must never be silently rebound to another note.
                    editor.selected = false;
                    editor.draft = Default::default();
                }
                editor.base = revision;
            }
        }
        ui.separator();
        ui.label(if editor.selected { "Selected note" } else { "New note" });
        ui.horizontal_wrapped(|ui| {
            ui.label("Beat");
            ui.add(egui::DragValue::new(&mut editor.draft.time).range(0.0..=120_000.0).speed(0.25));
            ui.label("Length");
            ui.add(egui::DragValue::new(&mut editor.draft.length).range(0.001..=120_000.0).speed(0.25));
            ui.label("Key");
            ui.add(egui::DragValue::new(&mut editor.draft.key).range(0..=127));
            ui.label("Velocity");
            ui.add(egui::DragValue::new(&mut editor.draft.velocity).range(1.0..=127.0));
            if editor.selected {
                ui.checkbox(&mut editor.draft.muted, "Muted");
            }
        });
        let mut action = None;
        ui.horizontal(|ui| {
            if editor.selected {
                if ui.add_enabled(!stale, egui::Button::new("Apply note")).clicked() {
                    let n = &editor.draft;
                    action = Some(WireEdit::NoteUpdate {
                        clip,
                        note: n.index,
                        time: n.time,
                        length: n.length,
                        key: n.key,
                        velocity: n.velocity,
                        muted: n.muted,
                    });
                }
                if ui.add_enabled(!stale, egui::Button::new("Delete note")).clicked() {
                    action = Some(WireEdit::NoteDelete { clip, note: editor.draft.index });
                }
                if ui.button("New note draft").clicked() {
                    editor.selected = false;
                    editor.draft = Default::default();
                    editor.base = revision;
                }
            } else if ui.add_enabled(!stale, egui::Button::new("Add note")).clicked() {
                let n = &editor.draft;
                action =
                    Some(WireEdit::NoteAdd { clip, time: n.time, length: n.length, key: n.key, velocity: n.velocity });
            }
        });
        if let Some(action) = action {
            let base = editor.base;
            if let Ok(edit) = serde_json::to_string(&action) {
                self.multiplayer_command("multiplayer.edit", json!({"base_revision":base,"edit":edit}));
            }
        }
    }
}
