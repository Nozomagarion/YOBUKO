// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Confirmed paged launcher grid with revision-bound scene and slot drafts.
use super::*;
use suhina_multiplayer::WireEdit;

#[derive(Default)]
pub(super) struct LauncherEditor {
    pub(super) base: Option<u64>,
    pub(super) selected: Option<(u32, usize)>,
    pub(super) scene: Option<usize>,
    pub(super) name: String,
    pub(super) source: Option<u32>,
    destination_track: u32,
    destination_scene: usize,
    length: f64,
    row_page: usize,
    track_page: usize,
}
impl LauncherEditor {
    pub(super) fn structure_changed(&mut self) {
        self.selected = None;
        self.scene = None;
        self.source = None;
        self.name.clear();
        self.base = None;
    }
}
impl App {
    pub(super) fn show_shared_launcher(&mut self, ui: &mut Ui) {
        let Some(id) = self.multiplayer.bound else { return };
        let Some(session) = self.projects.get(id) else { return };
        let project = session.project();
        let revision = self.multiplayer.controls.as_ref().map_or(0, |c| c.revision);
        let editor = &mut self.multiplayer.launcher;
        if editor.base.is_none() {
            editor.base = Some(revision);
            editor.length = 4.0;
        }
        let stale = editor.base != Some(revision);
        let editable = !stale && !self.multiplayer.submitted;
        ui.weak(
            "Shared organization · launches stay on your computer. Copy keeps the original; Move swaps occupied slots.",
        );
        if stale {
            ui.colored_label(theme::recording(), "New changes arrived. Refresh and select again before editing.");
            if ui.button("Refresh launcher draft").clicked() {
                editor.structure_changed();
            }
        }
        let columns = &project.launcher.columns;
        let scenes = &project.launcher.scenes;
        let row_pages = scenes.len().div_ceil(8).max(1);
        let track_pages = columns.len().div_ceil(8).max(1);
        editor.row_page = editor.row_page.min(row_pages - 1);
        editor.track_page = editor.track_page.min(track_pages - 1);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(editor.row_page > 0, egui::Button::new("Previous scenes")).clicked() {
                editor.row_page -= 1;
            }
            ui.label(format!("Scenes {} / {row_pages}", editor.row_page + 1));
            if ui.add_enabled(editor.row_page + 1 < row_pages, egui::Button::new("Next scenes")).clicked() {
                editor.row_page += 1;
            }
            if ui.add_enabled(editor.track_page > 0, egui::Button::new("Previous columns")).clicked() {
                editor.track_page -= 1;
            }
            ui.label(format!("Tracks {} / {track_pages}", editor.track_page + 1));
            if ui.add_enabled(editor.track_page + 1 < track_pages, egui::Button::new("Next columns")).clicked() {
                editor.track_page += 1;
            }
        });
        egui::ScrollArea::horizontal().id_salt("shared-launcher-scroll").show(ui, |ui| {
            egui::Grid::new("shared-launcher-grid").striped(true).show(ui, |ui| {
                ui.strong("Scene");
                for c in columns.iter().skip(editor.track_page * 8).take(8) {
                    let name = project.track(c.track).map_or("Track", |t| t.name.as_str());
                    ui.add_sized([120., 20.], egui::Label::new(name).truncate());
                }
                ui.end_row();
                for (index, scene) in scenes.iter().enumerate().skip(editor.row_page * 8).take(8) {
                    let row = index + 1;
                    if ui.selectable_label(editor.scene == Some(row), format!("Scene {row} · {}", scene.name)).clicked()
                    {
                        editor.scene = Some(row);
                        editor.selected = None;
                        editor.name = scene.name.clone();
                        editor.base = Some(revision);
                    }
                    for column in columns.iter().skip(editor.track_page * 8).take(8) {
                        let clip = column.slots.get(index).and_then(|s| s.clip.as_ref());
                        let name = clip.map(|c| format!("Clip #{}", c.id.0)).unwrap_or_else(|| "Empty".into());
                        if ui
                            .selectable_label(
                                editor.selected == Some((column.track.0, row)),
                                format!("Slot {}:{row} · {name}", column.track.0),
                            )
                            .on_hover_text(
                                clip.and_then(|c| project.content(c.content)).map_or("Empty slot", |c| c.name.as_str()),
                            )
                            .clicked()
                        {
                            editor.selected = Some((column.track.0, row));
                            editor.scene = Some(row);
                            editor.name = scene.name.clone();
                            editor.destination_track = column.track.0;
                            editor.destination_scene = row;
                            editor.base = Some(revision);
                        }
                    }
                    ui.end_row();
                }
            });
        });
        let mut edit = None;
        let mut local = None;
        ui.separator();
        let name_label = ui.label("Scene name");
        ui.add(egui::TextEdit::singleline(&mut editor.name).char_limit(256).desired_width(220.))
            .labelled_by(name_label.id);
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(editable && scenes.len() < 128, egui::Button::new("Append shared scene")).clicked() {
                edit = Some(WireEdit::SceneInsert { scene: scenes.len() + 1, name: editor.name.clone() });
            }
            if let Some(scene) = editor.scene {
                if ui.add_enabled(editable && scenes.len() < 128, egui::Button::new("Insert scene before")).clicked() {
                    edit = Some(WireEdit::SceneInsert { scene, name: editor.name.clone() });
                }
                if ui.add_enabled(editable, egui::Button::new("Rename shared scene")).clicked() {
                    edit = Some(WireEdit::SceneRename { scene, name: editor.name.clone() });
                }
                if ui.add_enabled(editable && scenes.len() < 128, egui::Button::new("Duplicate shared scene")).clicked()
                {
                    edit = Some(WireEdit::SceneDuplicate { scene });
                }
                if ui.add_enabled(editable && scenes.len() > 1, egui::Button::new("Delete shared scene")).clicked() {
                    edit = Some(WireEdit::SceneDelete { scene });
                }
                if ui
                    .add_enabled(
                        !stale && Some(id) == self.projects.active(),
                        egui::Button::new("Launch scene locally"),
                    )
                    .clicked()
                {
                    local = Some(Action::new("scene.launch", json!({"scene":scene})));
                }
            }
        });
        if let Some((track, scene)) = editor.selected {
            let clip = columns
                .iter()
                .find(|c| c.track.0 == track)
                .and_then(|c| c.slots.get(scene - 1))
                .and_then(|s| s.clip.as_ref());
            ui.separator();
            ui.label(format!("Selected slot · track {track} · scene {scene}"));
            if let Some(clip) = clip {
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(editable, egui::Button::new("Clear shared slot")).clicked() {
                        edit = Some(WireEdit::ClipDelete { clip: clip.id.0 });
                    }
                    if ui
                        .add_enabled(
                            !stale && Some(id) == self.projects.active(),
                            egui::Button::new("Launch slot locally"),
                        )
                        .clicked()
                    {
                        local = Some(Action::new("clip.launch", json!({"track":track,"scene":scene})));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_label("Destination track")
                        .selected_text(editor.destination_track.to_string())
                        .show_ui(ui, |ui| {
                            for c in columns {
                                let name = project.track(c.track).map_or("Track", |t| t.name.as_str());
                                ui.selectable_value(&mut editor.destination_track, c.track.0, name);
                            }
                        });
                    ui.label("Destination scene");
                    ui.add(egui::DragValue::new(&mut editor.destination_scene).range(1..=scenes.len()));
                    if ui.add_enabled(editable, egui::Button::new("Move or swap shared slot")).clicked() {
                        edit = Some(WireEdit::SlotMove {
                            track,
                            scene,
                            to_track: editor.destination_track,
                            to_scene: editor.destination_scene,
                        });
                    }
                });
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Clip length");
                    ui.add(egui::DragValue::new(&mut editor.length).range(0.001..=120_000.).speed(0.25));
                    if ui.add_enabled(editable, egui::Button::new("Create empty shared slot")).clicked() {
                        edit = Some(WireEdit::SlotCreate { track, scene, length: editor.length });
                    }
                });
                egui::ComboBox::from_label("Source clip")
                    .width(350.)
                    .selected_text(editor.source.map_or_else(|| "Choose a clip".into(), |s| format!("Clip #{s}")))
                    .show_ui(ui, |ui| {
                        for (source, place) in
                            project
                                .arrangement
                                .lanes
                                .iter()
                                .flat_map(|l| l.clips.iter().map(|c| (c, "arrangement")))
                                .chain(columns.iter().flat_map(|c| {
                                    c.slots.iter().filter_map(|s| s.clip.as_ref()).map(|c| (c, "launcher"))
                                }))
                        {
                            let name = project.content(source.content).map_or("Clip", |c| c.name.as_str());
                            ui.selectable_value(
                                &mut editor.source,
                                Some(source.id.0),
                                format!("Clip #{} · {name} · {place}", source.id.0),
                            );
                        }
                    });
                if ui
                    .add_enabled(editable && editor.source.is_some(), egui::Button::new("Copy clip into shared slot"))
                    .clicked()
                    && let Some(source) = editor.source
                {
                    edit = Some(WireEdit::SlotCopy { source, track, scene });
                }
                ui.weak("New clips start empty. MIDI notes edits launcher clips too. Copied audio keeps its verified sound.");
            }
        }
        ui.weak(format!("Confirmed revision {revision}. Each computer saves its own copy."));
        if let Some(action) = edit
            && let Ok(text) = serde_json::to_string(&action)
        {
            self.multiplayer_command("multiplayer.edit", json!({"base_revision":revision,"edit":text}));
        }
        if let Some(action) = local {
            self.run(&action);
        }
    }
}
