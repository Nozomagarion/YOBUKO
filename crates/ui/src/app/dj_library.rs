// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ space's library: every track, or one playlist's, searched by title, tempo or key and
//! sorted by any column; playlists made, filled and emptied here; Load A / Load B on each row.

use super::*;

/// What the library is sorted by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::app) enum Sort {
    /// The order the library keeps: when tracks came in, or the playlist's order.
    #[default]
    Added,
    Title,
    Bpm,
    Key,
    Length,
}

/// How the library shows: the search, the sort and the playlist.
#[derive(Default)]
pub(in crate::app) struct LibraryView {
    pub(in crate::app) search: String,
    pub(in crate::app) sort: Sort,
    pub(in crate::app) descending: bool,
    /// The playlist shown, by id; every track when `None`.
    pub(in crate::app) playlist: Option<u64>,
}

impl Decks {
    /// The tracks the library shows, in the order it shows them.
    pub(in crate::app) fn shown_tracks(&self) -> Vec<Value> {
        let all = self.snapshot["tracks"].as_array().cloned().unwrap_or_default();
        let view = &self.library;
        let mut tracks: Vec<Value> = match view.playlist {
            None => all,
            Some(id) => {
                let playlist =
                    self.snapshot["playlists"].as_array().into_iter().flatten().find(|p| p["id"].as_u64() == Some(id));
                playlist
                    .and_then(|p| p["tracks"].as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|t| all.iter().find(|track| track["id"] == *t).cloned())
                    .collect()
            }
        };
        let words: Vec<String> = view.search.split_whitespace().map(str::to_lowercase).collect();
        if !words.is_empty() {
            tracks.retain(|t| {
                let bpm = t["bpm"].as_f64().map(|b| format!("{b:.0}")).unwrap_or_default();
                let text = format!(
                    "{} {} {}",
                    t["title"].as_str().unwrap_or_default(),
                    t["key"].as_str().unwrap_or_default(),
                    bpm
                )
                .to_lowercase();
                words.iter().all(|w| text.contains(w.as_str()))
            });
        }
        let number = |t: &Value, key: &str| t[key].as_f64().unwrap_or(f64::MAX);
        let text = |t: &Value, key: &str| t[key].as_str().unwrap_or_default().to_lowercase();
        match view.sort {
            Sort::Added => {}
            Sort::Title => tracks.sort_by_key(|t| text(t, "title")),
            Sort::Key => tracks.sort_by_key(|t| text(t, "key")),
            Sort::Bpm => tracks.sort_by(|a, b| number(a, "bpm").total_cmp(&number(b, "bpm"))),
            Sort::Length => tracks.sort_by(|a, b| number(a, "seconds").total_cmp(&number(b, "seconds"))),
        }
        if view.descending {
            tracks.reverse();
        }
        tracks
    }

    fn playlists(&self) -> Vec<(u64, String, usize)> {
        self.snapshot["playlists"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                Some((p["id"].as_u64()?, p["name"].as_str()?.to_owned(), p["tracks"].as_array().map_or(0, Vec::len)))
            })
            .collect()
    }

    fn revision(&self) -> u64 {
        self.snapshot["revision"].as_u64().unwrap_or(0)
    }
}

impl App {
    /// The DJ library: the playlists on the left, the tracks on the right.
    pub(super) fn library_card(&mut self, ui: &mut Ui) {
        egui::Frame::NONE
            .fill(theme::panel_background())
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(CARD_RADIUS)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height().max(160.0) - 28.0);
                let total = self.dj.decks.snapshot["tracks"].as_array().map_or(0, Vec::len);
                let tracks = self.dj.decks.shown_tracks();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("LIBRARY").small().color(theme::text_hint()));
                    ui.label(egui::RichText::new(format!("{} of {total} tracks", tracks.len())).small().color(theme::text_hint()));
                    ui.add_space(GAP);
                    let name = ui.label(egui::RichText::new("Search").small().color(theme::text_hint()));
                    let search = egui::TextEdit::singleline(&mut self.dj.decks.library.search)
                        .hint_text("Title, key or BPM")
                        .desired_width(220.0);
                    let search = ui.add(search).labelled_by(name.id);
                    if search.changed() {
                        self.dj.decks.selected = 0;
                    }
                    if !self.dj.decks.library.search.is_empty()
                        && tools::small_button(ui, "Clear the search", "CLEAR", false, deck_color(0), true).clicked()
                    {
                        self.dj.decks.library.search.clear();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Drop audio files or folders anywhere on this page")
                                .small()
                                .color(theme::text_hint()),
                        );
                    });
                });
                let decks = &self.dj.decks;
                if !decks.imports.is_empty() || (decks.queued > 0 && decks.snapshot["busy"] == true) {
                    ui.label(
                        egui::RichText::new(format!(
                            "Analysing tracks for the library: {} of {}…",
                            decks.imported.min(decks.queued),
                            decks.queued
                        ))
                        .color(deck_color(0)),
                    );
                }
                if let Some(error) = self.dj.decks.error.clone() {
                    ui.colored_label(theme::recording(), error);
                }
                ui.add_space(theme::PAD);
                if total == 0 {
                    ui.add_space(theme::PAD * 3.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("Your DJ library is empty").strong().size(15.0));
                        ui.label(
                            egui::RichText::new(
                                "Drop audio files or a folder here, click Add my Music folder, or send a project you made.",
                            )
                            .color(theme::text_hint()),
                        );
                    });
                    return;
                }
                let height = ui.available_height().max(120.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    let down = egui::Layout::top_down(egui::Align::Min);
                    ui.allocate_ui_with_layout(Vec2::new(170.0, height), down, |ui| self.playlist_list(ui));
                    let rest = ui.available_width();
                    ui.allocate_ui_with_layout(Vec2::new(rest, height), down, |ui| self.track_table(ui, &tracks));
                });
            });
    }

    /// All tracks and the playlists, to choose what the table shows; New playlist under.
    fn playlist_list(&mut self, ui: &mut Ui) {
        ui.set_width(170.0);
        ui.label(egui::RichText::new("PLAYLISTS").small().color(theme::text_hint()));
        let current = self.dj.decks.library.playlist;
        let choose = |ui: &mut Ui, id: Option<u64>, text: String, label: String| {
            let response = ui.add_sized(
                Vec2::new(ui.available_width(), 24.0),
                egui::Button::selectable(current == id, egui::RichText::new(text).size(12.5)).corner_radius(8),
            );
            response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, current == id, &label));
            response
        };
        let mut picked = choose(ui, None, "All tracks".into(), "Show all tracks".into()).clicked().then_some(None);
        let playlists = self.dj.decks.playlists();
        let mut delete = None;
        egui::ScrollArea::vertical().id_salt("dj-playlists").max_height(ui.available_height() - 34.0).show(ui, |ui| {
            for (id, name, count) in &playlists {
                let response = choose(ui, Some(*id), format!("{name}  ·  {count}"), format!("Show playlist {name}"));
                if response.clicked() {
                    picked = Some(Some(*id));
                }
                response.context_menu(|ui| {
                    if ui.button(format!("Delete playlist {name}")).clicked() {
                        delete = Some(*id);
                        ui.close();
                    }
                });
            }
        });
        if let Some(id) = picked {
            self.dj.decks.library.playlist = id;
            self.dj.decks.selected = 0;
        }
        if let Some(id) = delete {
            let revision = self.dj.decks.revision();
            if self.deck_command("dj.playlist.delete", json!({"playlist": id, "revision": revision}))
                && self.dj.decks.library.playlist == Some(id)
            {
                self.dj.decks.library.playlist = None;
            }
        }
        let new = ui.add(egui::Button::new("+ New playlist").corner_radius(BUTTON_RADIUS));
        if new.clicked() {
            let taken: Vec<String> = playlists.iter().map(|(_, name, _)| name.clone()).collect();
            let name = (1..).map(|n| format!("Playlist {n}")).find(|n| !taken.contains(n)).unwrap_or_default();
            let revision = self.dj.decks.revision();
            self.deck_command("dj.playlist.create", json!({"name": name, "revision": revision}));
        }
    }

    /// The tracks, with their column headings to sort by.
    fn track_table(&mut self, ui: &mut Ui, tracks: &[Value]) {
        let row = 28.0;
        let numbers = 150.0;
        let buttons = 2.0 * 64.0 + 8.0;
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let title_width = (ui.available_width() - buttons - numbers).max(60.0);
            let columns = [
                (Sort::Title, "TITLE", title_width),
                (Sort::Bpm, "BPM", 54.0),
                (Sort::Key, "KEY", 40.0),
                (Sort::Length, "TIME", 52.0),
            ];
            for (sort, text, width) in columns {
                let view = &self.dj.decks.library;
                let arrow = match (view.sort == sort, view.descending) {
                    (true, false) => " ▲",
                    (true, true) => " ▼",
                    (false, _) => "",
                };
                let heading =
                    egui::Button::new(egui::RichText::new(format!("{text}{arrow}")).small().color(theme::text_hint()))
                        .frame(false)
                        .min_size(Vec2::new(width, 18.0));
                let response = ui.add(heading);
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Sort by {}", text.to_lowercase()))
                });
                if response.clicked() {
                    let view = &mut self.dj.decks.library;
                    // Ascending, descending, then back to the library's own order.
                    (view.sort, view.descending) = match (view.sort == sort, view.descending) {
                        (false, _) => (sort, false),
                        (true, false) => (sort, true),
                        (true, true) => (Sort::Added, false),
                    };
                }
            }
        });
        if tracks.is_empty() {
            ui.add_space(theme::PAD * 2.0);
            ui.label(egui::RichText::new("No track matches.").color(theme::text_hint()));
            return;
        }
        egui::ScrollArea::vertical().id_salt("dj-library").auto_shrink([false, true]).show_rows(
            ui,
            row,
            tracks.len(),
            |ui, rows| {
                for index in rows {
                    if let Some(track) = tracks.get(index) {
                        self.library_row(ui, track, index, row);
                    }
                }
            },
        );
    }

    fn library_row(&mut self, ui: &mut Ui, track: &Value, index: usize, height: f32) {
        let title = track["title"].as_str().unwrap_or("Untitled");
        let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
        if response.clicked() {
            self.dj.decks.selected = index;
        }
        self.row_menu(&response, track, title);
        if index == self.dj.decks.selected {
            ui.painter().rect_filled(rect, CornerRadius::same(8), theme::control_hover());
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(8),
                Stroke::new(1.0, theme::border()),
                StrokeKind::Inside,
            );
        } else if index % 2 == 1 {
            ui.painter().rect_filled(rect, CornerRadius::same(8), theme::control_background().gamma_multiply(0.5));
        }
        let mut row = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(8.0, 0.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let buttons = 2.0 * 64.0 + 8.0;
        let numbers = 150.0;
        let title_width = (row.available_width() - buttons - numbers).max(60.0);
        let line = egui::Layout::left_to_right(egui::Align::Center);
        row.allocate_ui_with_layout(Vec2::new(title_width, height), line, |ui| {
            ui.set_min_width(title_width);
            ui.add(egui::Label::new(egui::RichText::new(title).size(13.0)).truncate());
        });
        row.allocate_ui_with_layout(Vec2::new(numbers, height), line, |ui| {
            ui.set_min_width(numbers);
            let bpm = track["bpm"].as_f64().map_or_else(|| "—".to_owned(), |b| format!("{b:.1}"));
            let key = track["key"].as_str().unwrap_or("");
            let length = clock(track["seconds"].as_f64().unwrap_or(0.0));
            let length = length.split('.').next().unwrap_or_default().to_owned();
            ui.label(egui::RichText::new(format!("{bpm:>6}  {key:<4} {length:>6}")).font(theme::mono_font(12.0)));
        });
        for (deck, name) in NAMES.into_iter().enumerate() {
            let busy = self.dj.decks.playing(deck);
            let load = row
                .add_enabled(
                    !busy,
                    egui::Button::new(egui::RichText::new(format!("Load {name}")).color(Color32::WHITE).size(12.0))
                        .fill(deck_color(deck))
                        .corner_radius(BUTTON_RADIUS)
                        .min_size(Vec2::new(60.0, 22.0)),
                )
                .on_disabled_hover_text(format!("Deck {name} is playing: pause it first"));
            load.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, !busy, format!("Load {title} on deck {name}"))
            });
            if load.clicked() {
                self.dj.decks.selected = index;
                self.load_selected(deck);
            }
        }
    }

    /// A row's right-click menu: load it, add it to a playlist, take it out of the one shown.
    fn row_menu(&mut self, response: &egui::Response, track: &Value, title: &str) {
        let Some(id) = track["id"].as_u64() else { return };
        let playlists = self.dj.decks.playlists();
        let shown = self.dj.decks.library.playlist;
        let mut action: Option<(&str, Value)> = None;
        let mut load = None;
        let mut sampler = None;
        response.context_menu(|ui| {
            ui.label(egui::RichText::new(title).strong());
            for (deck, name) in NAMES.into_iter().enumerate() {
                if ui
                    .add_enabled(!self.dj.decks.playing(deck), egui::Button::new(format!("Load on deck {name}")))
                    .clicked()
                {
                    load = Some(deck);
                    ui.close();
                }
            }
            ui.menu_button("Load in the sampler", |ui| {
                for slot in 0..super::sampler::SLOTS {
                    if ui.button(format!("Slot {}", slot + 1)).clicked() {
                        sampler = Some(slot);
                        ui.close();
                    }
                }
            });
            ui.separator();
            ui.menu_button("Add to playlist", |ui| {
                if playlists.is_empty() {
                    ui.label(
                        egui::RichText::new("No playlist yet: make one with + New playlist").color(theme::text_hint()),
                    );
                }
                for (playlist, name, _) in &playlists {
                    if ui.button(name).clicked() {
                        action = Some(("dj.playlist.addTrack", json!({"playlist": playlist, "track": id})));
                        ui.close();
                    }
                }
            });
            if let Some(playlist) = shown
                && ui.button("Remove from this playlist").clicked()
            {
                action = Some(("dj.playlist.removeTrack", json!({"playlist": playlist, "track": id})));
                ui.close();
            }
        });
        if let Some(deck) = load
            && !self.dj.decks.playing(deck)
            && self.deck_command("dj.deck.load", json!({"deck": NAMES[deck], "track": id}))
        {
            self.dj.decks.cue[deck] = 0.0;
            self.dj.decks.set(Some(deck), "loading", json!(true));
        }
        if let Some(slot) = sampler {
            self.load_sampler(slot, id);
        }
        if let Some((command, mut params)) = action {
            params["revision"] = json!(self.dj.decks.revision());
            self.deck_command(command, params);
        }
    }
}
