// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ space's library: every track, or one playlist's, crate's or day of history's,
//! searched by title, tempo or key and sorted by any column; playlists and crates made, filled
//! and emptied here; Load A / Load B on each row.

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
    /// The playlist or crate shown, by id; every track when `None`.
    pub(in crate::app) playlist: Option<u64>,
    /// The day of history shown, instead.
    pub(in crate::app) day: Option<String>,
    /// The Auto DJ queue shown, instead.
    pub(in crate::app) auto: bool,
}

/// A list in the library's side bar: a playlist or a crate.
pub(in crate::app) struct List {
    pub(in crate::app) id: u64,
    pub(in crate::app) name: String,
    pub(in crate::app) count: usize,
    pub(in crate::app) is_crate: bool,
}

impl Decks {
    /// The tracks the library shows, in the order it shows them.
    pub(in crate::app) fn shown_tracks(&self) -> Vec<Value> {
        let all = self.snapshot["tracks"].as_array().cloned().unwrap_or_default();
        let view = &self.library;
        let by_id = |ids: Vec<&Value>| -> Vec<Value> {
            ids.into_iter().filter_map(|t| all.iter().find(|track| track["id"] == *t).cloned()).collect()
        };
        let mut tracks: Vec<Value> = match (&view.day, view.playlist) {
            _ if view.auto => {
                let queue: Vec<Value> = self.auto_queue().into_iter().map(Value::from).collect();
                by_id(queue.iter().collect())
            }
            (Some(day), _) => {
                let days = self.snapshot["history"]["days"].as_array().cloned().unwrap_or_default();
                let plays = days.iter().find(|d| d["day"] == day.as_str()).and_then(|d| d["plays"].as_array().cloned());
                let plays = plays.unwrap_or_default();
                by_id(plays.iter().map(|p| &p["track"]).collect())
            }
            (None, None) => all.clone(),
            (None, Some(id)) => {
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

    /// The playlists and crates.
    pub(in crate::app) fn lists(&self) -> Vec<List> {
        self.snapshot["playlists"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                Some(List {
                    id: p["id"].as_u64()?,
                    name: p["name"].as_str()?.to_owned(),
                    count: p["tracks"].as_array().map_or(0, Vec::len),
                    is_crate: p["kind"] == "crate",
                })
            })
            .collect()
    }

    /// The days of history, newest first, with how many tracks were played.
    fn days(&self) -> Vec<(String, usize)> {
        self.snapshot["history"]["days"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| Some((d["day"].as_str()?.to_owned(), d["plays"].as_array().map_or(0, Vec::len))))
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
                    if self.dj.decks.library.auto || self.dj.decks.auto()["on"] == true {
                        ui.add_space(GAP);
                        self.auto_dj_bar(ui);
                    } else {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new("Drop audio files or folders anywhere on this page")
                                    .small()
                                    .color(theme::text_hint()),
                            );
                        });
                    }
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

    /// All tracks, the playlists, the crates and the days of history, to choose what the table
    /// shows; New playlist and New crate above them.
    fn playlist_list(&mut self, ui: &mut Ui) {
        ui.set_width(170.0);
        let view = &self.dj.decks.library;
        let (current, day) = (view.playlist, view.day.clone());
        let choose = |ui: &mut Ui, on: bool, text: String, label: String| {
            let response = ui.add_sized(
                Vec2::new(ui.available_width(), 24.0),
                egui::Button::selectable(on, egui::RichText::new(text).size(12.5)).corner_radius(8),
            );
            response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &label));
            response
        };
        let heading = |ui: &mut Ui, text: &str| {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(text).small().color(theme::text_hint()));
        };
        let auto = view.auto;
        let all_on = current.is_none() && day.is_none() && !auto;
        let mut picked: Option<(Option<u64>, Option<String>)> =
            choose(ui, all_on, "All tracks".into(), "Show all tracks".into()).clicked().then_some((None, None));
        let queued = self.dj.decks.auto_queue().len();
        let mut show_auto = choose(ui, auto, format!("Auto DJ  ·  {queued}"), "Show the Auto DJ list".into()).clicked();
        let lists = self.dj.decks.lists();
        let days = self.dj.decks.days();
        let taken: Vec<String> = lists.iter().map(|l| l.name.clone()).collect();
        let free = |word: &str| (1..).map(|n| format!("{word} {n}")).find(|n| !taken.contains(n)).unwrap_or_default();
        let mut make = None;
        ui.horizontal(|ui| {
            let new = [
                ("+ New playlist", "+ PLAYLIST", "Playlist", "dj.playlist.create"),
                ("+ New crate", "+ CRATE", "Crate", "dj.crate.create"),
            ];
            for (label, text, word, command) in new {
                if tools::small_button(ui, label, text, false, deck_color(0), true).clicked() {
                    make = Some((command, free(word)));
                }
            }
        });
        let mut delete = None;
        let mut forget = None;
        egui::ScrollArea::vertical().id_salt("dj-playlists").max_height(ui.available_height()).show(ui, |ui| {
            for crates in [false, true] {
                heading(ui, if crates { "CRATES" } else { "PLAYLISTS" });
                for list in lists.iter().filter(|l| l.is_crate == crates) {
                    let (kind, name) = (if crates { "crate" } else { "playlist" }, &list.name);
                    let on = current == Some(list.id) && day.is_none() && !auto;
                    let response = choose(ui, on, format!("{name}  ·  {}", list.count), format!("Show {kind} {name}"));
                    if response.clicked() {
                        picked = Some((Some(list.id), None));
                    }
                    response.context_menu(|ui| {
                        if ui.button(format!("Delete {kind} {name}")).clicked() {
                            delete = Some(list.id);
                            ui.close();
                        }
                    });
                }
            }
            if !days.is_empty() {
                heading(ui, "HISTORY");
            }
            for (d, count) in &days {
                let on = day.as_deref() == Some(d.as_str()) && !auto;
                let response = choose(ui, on, format!("{d}  ·  {count}"), format!("Show the history of {d}"));
                if response.clicked() {
                    picked = Some((None, Some(d.clone())));
                }
                response.context_menu(|ui| {
                    if ui.button(format!("Forget the history of {d}")).clicked() {
                        forget = Some(d.clone());
                        ui.close();
                    }
                });
            }
        });
        if let Some((command, name)) = make {
            let revision = self.dj.decks.revision();
            self.deck_command(command, json!({"name": name, "revision": revision}));
        }
        if let Some((list, day)) = picked {
            self.dj.decks.library.playlist = list;
            self.dj.decks.library.day = day;
            self.dj.decks.library.auto = false;
            self.dj.decks.selected = 0;
            show_auto = false;
        }
        if show_auto {
            let view = &mut self.dj.decks.library;
            (view.playlist, view.day, view.auto) = (None, None, true);
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
        if let Some(d) = forget
            && self.deck_command("dj.history.delete", json!({"day": d}))
            && self.dj.decks.library.day.as_ref() == Some(&d)
        {
            self.dj.decks.library.day = None;
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
        let lists = self.dj.decks.lists();
        let view = &self.dj.decks.library;
        let shown = view.playlist.filter(|_| view.day.is_none() && !view.auto);
        let in_queue = view.auto.then(|| self.dj.decks.auto_queue().iter().position(|t| *t == id)).flatten();
        let mut queue = false;
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
            if ui.button("Add to Auto DJ").clicked() {
                queue = true;
                ui.close();
            }
            if let Some(at) = in_queue
                && ui.button("Remove from Auto DJ").clicked()
            {
                action = Some(("dj.autodj.remove", json!({"position": at + 1})));
                ui.close();
            }
            for crates in [false, true] {
                let (menu, none) = if crates {
                    ("Add to crate", "No crate yet: make one with + New crate")
                } else {
                    ("Add to playlist", "No playlist yet: make one with + New playlist")
                };
                ui.menu_button(menu, |ui| {
                    let mut any = false;
                    for list in lists.iter().filter(|l| l.is_crate == crates) {
                        any = true;
                        if ui.button(&list.name).clicked() {
                            action = Some(("dj.playlist.addTrack", json!({"playlist": list.id, "track": id})));
                            ui.close();
                        }
                    }
                    if !any {
                        ui.label(egui::RichText::new(none).color(theme::text_hint()));
                    }
                });
            }
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
        if queue {
            self.queue_auto_dj(id);
        }
        if let Some((command, mut params)) = action {
            params["revision"] = json!(self.dj.decks.revision());
            self.deck_command(command, params);
        }
    }
}
