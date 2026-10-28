// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::{PlaylistDraft, *};
const PAGE: usize = 24;

fn pages(ui: &mut Ui, page: &mut usize, count: usize, salt: &str) {
    let last = count.saturating_sub(1) / PAGE;
    *page = (*page).min(last);
    ui.push_id(salt, |ui| {
        ui.horizontal(|ui| {
            if ui.add_enabled(*page > 0, egui::Button::new("Previous page")).clicked() {
                *page = page.saturating_sub(1);
            }
            ui.label(format!("Page {} / {} · {count} tracks", *page + 1, last + 1));
            if ui.add_enabled(*page < last, egui::Button::new("Next page")).clicked() {
                *page += 1;
            }
        })
    });
}
fn action(ui: &mut Ui, text: &str, label: String, enabled: bool) -> bool {
    let response = ui.add_enabled(enabled, egui::Button::new(text));
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label.as_str()));
    response.clicked()
}

impl App {
    pub(super) fn show_dj_playlists(&mut self, ui: &mut Ui, busy: bool) {
        let playlists = self.dj.snapshot["playlists"].as_array().cloned().unwrap_or_default();
        let revision = self.dj.snapshot["revision"].as_u64().unwrap_or(0);
        if !busy && self.dj.selected_playlist.is_some_and(|id| !playlists.iter().any(|p| p["id"].as_u64() == Some(id)))
        {
            self.dj.selected_playlist = None;
            self.dj.draft = None;
        }
        if self.dj.delete_confirmation.is_some_and(|(_, base)| base != revision) {
            self.dj.delete_confirmation = None;
        }
        let old = self.dj.selected_playlist;
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                let title = playlists
                    .iter()
                    .find(|p| p["id"].as_u64() == self.dj.selected_playlist)
                    .and_then(|p| p["name"].as_str())
                    .unwrap_or("All tracks");
                egui::ComboBox::from_label("DJ collection").selected_text(title).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.dj.selected_playlist, None, "All tracks");
                    for p in &playlists {
                        if let Some(id) = p["id"].as_u64() {
                            ui.selectable_value(
                                &mut self.dj.selected_playlist,
                                Some(id),
                                format!("{} · #{id}", p["name"].as_str().unwrap_or("Playlist")),
                            );
                        }
                    }
                });
                let label = ui.label("New playlist name");
                ui.add(egui::TextEdit::singleline(&mut self.dj.playlist_name).char_limit(128).desired_width(160.0))
                    .labelled_by(label.id);
                if ui
                    .add_enabled(
                        !self.dj.playlist_name.trim().is_empty() && playlists.len() < 128,
                        egui::Button::new("Create playlist"),
                    )
                    .clicked()
                {
                    self.dj_command("dj.playlist.create", json!({"name": self.dj.playlist_name, "revision": revision}));
                }
            });
        });
        if old != self.dj.selected_playlist {
            self.dj.draft = None;
            self.dj.delete_confirmation = None;
            self.dj.playlist_page = 0;
            self.dj.library_page = 0;
        }
        let tracks = self.dj.snapshot["tracks"].as_array().cloned().unwrap_or_default();
        let selected = playlists.iter().find(|p| p["id"].as_u64() == self.dj.selected_playlist);
        if let Some(p) = selected {
            let id = p["id"].as_u64().unwrap_or(0);
            let name = p["name"].as_str().unwrap_or_default();
            self.dj_playlist_name(ui, id, name, revision, busy);
            let ids = p["tracks"].as_array().cloned().unwrap_or_default();
            let ordered: Vec<Value> =
                ids.iter().filter_map(|id| tracks.iter().find(|t| t["id"] == *id).cloned()).collect();
            ui.strong(format!("Set order · {}", name));
            if ordered.is_empty() {
                ui.label("This playlist is empty. Add tracks from the library below.");
            }
            pages(ui, &mut self.dj.playlist_page, ordered.len(), "dj-set-pages");
            self.dj_track_rows(ui, &ordered, Some((id, revision)), false, &ids, busy);
            ui.separator();
            ui.strong("Add from library");
            pages(ui, &mut self.dj.library_page, tracks.len(), "dj-add-pages");
            self.dj_track_rows(ui, &tracks, Some((id, revision)), true, &ids, busy);
        } else {
            if tracks.is_empty() && !busy {
                ui.label("Import an audio file, or open a project and render it into this library.");
            }
            pages(ui, &mut self.dj.library_page, tracks.len(), "dj-library-pages");
            self.dj_track_rows(ui, &tracks, None, false, &[], busy);
        }
    }

    fn dj_playlist_name(&mut self, ui: &mut Ui, id: u64, name: &str, revision: u64, busy: bool) {
        if self.dj.draft.as_ref().is_none_or(|d| d.id != id || (d.name == d.original && d.revision != revision)) {
            self.dj.draft = Some(PlaylistDraft { id, name: name.into(), original: name.into(), revision });
        }
        let mut rename = None;
        let mut refresh = false;
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                let Some(draft) = self.dj.draft.as_mut() else { return };
                let label = ui.label("Rename playlist");
                ui.add(egui::TextEdit::singleline(&mut draft.name).char_limit(128).desired_width(180.0))
                    .labelled_by(label.id);
                let stale = draft.revision != revision;
                if ui
                    .add_enabled(!stale && !draft.name.trim().is_empty(), egui::Button::new("Save playlist name"))
                    .clicked()
                {
                    rename = Some((draft.revision, draft.name.clone()));
                }
                if stale {
                    ui.label("Library changed; refresh this draft before renaming.");
                    refresh = ui.button("Refresh playlist draft").clicked();
                }
                if ui.button("Delete playlist…").clicked() {
                    self.dj.delete_confirmation = Some((id, revision));
                }
            });
        });
        if refresh {
            self.dj.draft = None;
        }
        if let Some((base, name)) = rename {
            self.dj_command("dj.playlist.rename", json!({"playlist": id, "name": name, "revision": base}));
            // Clear only after successful completion; a failed write keeps the typed name.
        }
        if self.dj.draft.as_ref().is_some_and(|d| d.revision != revision && d.name.trim() == name) {
            self.dj.draft = None;
        }
        if self.dj.delete_confirmation == Some((id, revision)) {
            ui.horizontal(|ui| {
                ui.label(format!("Delete “{name}”? Audio files stay in the library."));
                if ui.add_enabled(!busy, egui::Button::new("Confirm delete playlist")).clicked() {
                    self.dj_command("dj.playlist.delete", json!({"playlist": id, "revision": revision}));
                }
                if ui.button("Cancel playlist deletion").clicked() {
                    self.dj.delete_confirmation = None;
                }
            });
        }
    }

    fn dj_track_rows(
        &mut self,
        ui: &mut Ui,
        tracks: &[Value],
        playlist: Option<(u64, u64)>,
        adding: bool,
        included: &[Value],
        busy: bool,
    ) {
        let page = if playlist.is_some() && !adding { self.dj.playlist_page } else { self.dj.library_page };
        let salt = if adding { "dj-add" } else { "dj-set" };
        egui::ScrollArea::vertical().id_salt(salt).max_height(if playlist.is_some() { 200.0 } else { 470.0 }).show(
            ui,
            |ui| {
                egui::Grid::new(salt).striped(true).spacing(Vec2::new(14.0, 8.0)).show(ui, |ui| {
                    for text in ["Track", "BPM", "Key", "Length", "Source"] {
                        ui.strong(text);
                    }
                    if playlist.is_some() {
                        ui.strong("Playlist");
                    }
                    ui.end_row();
                    for (index, track) in tracks.iter().enumerate().skip(page.saturating_mul(PAGE)).take(PAGE) {
                        let id = track["id"].as_u64().unwrap_or(0);
                        ui.push_id(id, |ui| {
                            ui.label(track["title"].as_str().unwrap_or("Untitled"))
                                .on_hover_text(track["path"].as_str().unwrap_or_default());
                        });
                        ui.label(track["bpm"].as_f64().map_or_else(|| "—".into(), |b| format!("{b:.1}")));
                        ui.label(track["key"].as_str().unwrap_or("—"));
                        let seconds = track["seconds"].as_f64().unwrap_or(0.0) as u64;
                        ui.label(format!("{}:{:02}", seconds / 60, seconds % 60));
                        if let Some(source) = track["source_project"].as_str() {
                            if ui.button("Open source project").clicked() {
                                self.dj.active = false;
                                self.request_open(PathBuf::from(source));
                            }
                        } else {
                            ui.label("Imported audio");
                        }
                        if let Some((playlist, revision)) = playlist {
                            ui.horizontal(|ui| {
                                let params = json!({"playlist": playlist, "track": id, "revision": revision});
                                if adding {
                                    let present = included.contains(&track["id"]);
                                    if action(
                                        ui,
                                        if present { "Added" } else { "Add" },
                                        format!("Add track {id}"),
                                        !busy && !present,
                                    ) {
                                        self.dj_command("dj.playlist.addTrack", params);
                                    }
                                } else {
                                    if action(ui, "↑", format!("Move track {id} up"), !busy && index > 0) {
                                        let mut p = params.clone();
                                        p["position"] = json!(index);
                                        self.dj_command("dj.playlist.moveTrack", p);
                                    }
                                    if action(
                                        ui,
                                        "↓",
                                        format!("Move track {id} down"),
                                        !busy && index + 1 < tracks.len(),
                                    ) {
                                        let mut p = params.clone();
                                        p["position"] = json!(index + 2);
                                        self.dj_command("dj.playlist.moveTrack", p);
                                    }
                                    if action(ui, "Remove", format!("Remove track {id}"), !busy) {
                                        self.dj_command("dj.playlist.removeTrack", params);
                                    }
                                }
                            });
                        }
                        ui.end_row();
                    }
                });
            },
        );
    }
}
