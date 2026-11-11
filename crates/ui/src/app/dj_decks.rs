// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ space's own mixer, drawn in YOBUKO's window: two decks (A and B) with their
//! waveforms, Play, Cue, Sync and tempo, a mixer with a three-band EQ, a filter, a level and a
//! headphone cue per deck and a crossfader, and the DJ library below to load them from. It
//! drives the session's `dj.*` commands; the sound is the engine's two decks
//! (`suhina_engine::decks`), on the same audio output as the studio.

use super::*;
use crate::kit::piou::{self, Mood};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use suhina_media::waveform::{Lane, Peaks};

/// The decks' names, as the commands take them.
pub(super) const NAMES: [&str; 2] = ["A", "B"];
/// How often the page reads the decks while one plays, and while the library works.
const PLAYING_POLL: Duration = Duration::from_millis(100);
const BUSY_POLL: Duration = Duration::from_millis(250);
/// One frame of a playing deck's waveform.
const FRAME: Duration = Duration::from_millis(33);
/// The seconds of a track a deck's close waveform shows.
const CLOSE_SECONDS: f64 = 8.0;
/// The most audio files one look into a folder adds, and the most entries it reads.
const MAX_FOUND: usize = 1000;
const MAX_LOOKED_AT: usize = 20_000;
/// The mixer's width between the decks.
const MIXER_WIDTH: f32 = 236.0;
/// The space between the cards.
const GAP: f32 = 12.0;
/// The corners of the DJ space's cards and buttons.
const CARD_RADIUS: u8 = 14;
const BUTTON_RADIUS: u8 = 10;

/// A deck's colour: A in the DJ space's pink, B in mint.
pub(super) fn deck_color(deck: usize) -> Color32 {
    if deck == 0 { Color32::from_rgb(0xDB, 0x27, 0x77) } else { Color32::from_rgb(0x10, 0xB9, 0x81) }
}

/// What the page knows of the decks and the library, between two reads.
#[derive(Default)]
pub(in crate::app) struct Decks {
    /// The last `dj.library` reply.
    pub(in crate::app) snapshot: Value,
    /// When it was read, to move a playing deck's playhead on between two reads.
    read_at: Option<Instant>,
    /// The decks have been used.
    pub(in crate::app) owned: bool,
    /// Audio files waiting to be analysed into the library, one at a time, with the project
    /// each was rendered from.
    pub(in crate::app) imports: VecDeque<(PathBuf, Option<PathBuf>)>,
    /// Of the files given since the queue was last empty, how many went in.
    imported: usize,
    queued: usize,
    /// Where each deck's Cue button goes back to, in seconds.
    cue: [f64; 2],
    /// What went wrong last, said on the page.
    pub(in crate::app) error: Option<String>,
}

impl Decks {
    fn deck(&self, deck: usize) -> &Value {
        &self.snapshot["performance"]["decks"][deck]
    }

    fn playing(&self, deck: usize) -> bool {
        self.deck(deck)["playing"] == true
    }

    fn any_playing(&self) -> bool {
        (0..2).any(|d| self.playing(d))
    }

    fn busy(&self) -> bool {
        self.snapshot["busy"] == true || !self.imports.is_empty() || (0..2).any(|d| self.deck(d)["loading"] == true)
    }

    fn number(&self, deck: usize, key: &str, default: f64) -> f64 {
        self.deck(deck)[key].as_f64().unwrap_or(default)
    }

    /// Seconds into the deck's track, moved on since the last read while it plays.
    fn position(&self, deck: usize) -> f64 {
        let at = self.number(deck, "position", 0.0);
        let length = self.number(deck, "seconds", 0.0);
        if !self.playing(deck) {
            return at;
        }
        let since = self.read_at.map_or(0.0, |t| t.elapsed().as_secs_f64().min(0.5));
        (at + since * self.number(deck, "rate", 1.0)).min(length)
    }

    /// The deck's tempo as it plays: the track's BPM times its speed.
    fn bpm(&self, deck: usize) -> Option<f64> {
        self.deck(deck)["bpm"].as_f64().map(|bpm| bpm * self.number(deck, "rate", 1.0))
    }

    /// Writes a value into the last read, so the page shows a change before the next read.
    fn set(&mut self, deck: Option<usize>, key: &str, value: Value) {
        let target = match deck {
            Some(deck) => &mut self.snapshot["performance"]["decks"][deck],
            None => &mut self.snapshot["performance"],
        };
        if target.is_object() {
            target[key] = value;
        }
    }

    /// Adds audio files to the queue of files to analyse.
    pub(in crate::app) fn add(&mut self, files: impl IntoIterator<Item = (PathBuf, Option<PathBuf>)>) {
        if self.imports.is_empty() {
            self.imported = 0;
            self.queued = 0;
        }
        for file in files {
            if !self.imports.iter().any(|(queued, _)| *queued == file.0) {
                self.imports.push_back(file);
                self.queued += 1;
            }
        }
    }

    /// The library's tracks' files, to leave out those already in it.
    fn known(&self) -> std::collections::HashSet<PathBuf> {
        self.snapshot["tracks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|t| t["path"].as_str().map(PathBuf::from))
            .collect()
    }
}

/// Whether YOBUKO's DJ library takes this file: a WAV, AIFF, FLAC, MP3 or Ogg file.
fn is_audio(path: &Path) -> bool {
    suhina_media::info::kind_of(path) == Some(suhina_media::info::FileKind::Sample)
}

/// The audio files in `folder` and below, hidden folders left out, at most [`MAX_FOUND`].
pub(super) fn audio_in(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut looked_at = 0;
    let mut folders = vec![folder.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            looked_at += 1;
            if looked_at > MAX_LOOKED_AT || found.len() >= MAX_FOUND {
                return found;
            }
            if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                continue;
            }
            if path.is_dir() {
                folders.push(path);
            } else if is_audio(&path) {
                found.push(path);
            }
        }
    }
    found
}

/// This person's Music folder, where it is.
fn music_folder() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let music = PathBuf::from(home).join("Music");
    music.is_dir().then_some(music)
}

/// `m:ss` for a time in seconds.
fn clock(seconds: f64) -> String {
    let seconds = seconds.max(0.0);
    let whole = seconds.floor() as u64;
    format!("{}:{:02}.{}", whole / 60, whole % 60, ((seconds - seconds.floor()) * 10.0).floor() as u64)
}

/// The EQ knob's place (0 … 1, the middle flat) for a band's dB, and back. Below the middle it
/// falls to a kill along a curve, so the first half of a turn trims and the last cuts.
fn knob_of_db(db: f64) -> f32 {
    let db = db.clamp(f64::from(suhina_engine::DECK_KILL_DB), f64::from(suhina_engine::DECK_BOOST_DB));
    if db >= 0.0 {
        (0.5 + 0.5 * db / f64::from(suhina_engine::DECK_BOOST_DB)) as f32
    } else {
        (0.5 - 0.5 * (db / f64::from(suhina_engine::DECK_KILL_DB)).sqrt()) as f32
    }
}

fn db_of_knob(knob: f32) -> f64 {
    let knob = f64::from(knob.clamp(0.0, 1.0));
    if knob >= 0.5 {
        f64::from(suhina_engine::DECK_BOOST_DB) * (knob - 0.5) / 0.5
    } else {
        f64::from(suhina_engine::DECK_KILL_DB) * ((0.5 - knob) / 0.5).powi(2)
    }
}

impl App {
    /// Reads the decks and the library when it is time, sends the next file to analyse, and
    /// asks for the frames a playing deck needs.
    pub(in crate::app) fn poll_decks(&mut self, ctx: &egui::Context) {
        let decks = &self.dj.decks;
        let every = if decks.any_playing() {
            Some(PLAYING_POLL)
        } else if decks.busy() {
            Some(BUSY_POLL)
        } else {
            None
        };
        let due = decks.snapshot.is_null()
            || match (every, decks.read_at) {
                (Some(every), Some(at)) => at.elapsed() >= every,
                (Some(_), None) => true,
                (None, _) => false,
            };
        if due {
            match self.projects.execute("dj.library", &json!({})) {
                Ok(snapshot) => {
                    if let Some(error) = snapshot["error"].as_str() {
                        self.dj.decks.error = Some(error.to_owned());
                    }
                    self.dj.decks.snapshot = snapshot;
                    self.dj.decks.read_at = Some(Instant::now());
                }
                Err(e) => {
                    self.dj.decks.error = Some(e.to_string());
                    self.dj.decks.imports.clear();
                    self.dj.decks.snapshot = json!({"error": e.to_string()});
                    self.dj.decks.read_at = Some(Instant::now());
                }
            }
            if self.dj.decks.snapshot["busy"] != true
                && let Some((file, project)) = self.dj.decks.imports.pop_front()
            {
                let params = json!({"path": file.display().to_string(), "sourceProject": project.map(|p| p.display().to_string())});
                match self.projects.execute("dj.import", &params) {
                    Ok(_) => {
                        self.dj.decks.imported += 1;
                        self.dj.decks.snapshot["busy"] = json!(true);
                    }
                    Err(e) => {
                        let name =
                            file.file_name().map_or_else(|| file.display().to_string(), |n| n.to_string_lossy().into());
                        self.dj.decks.error = Some(format!("{name}: {e}"));
                    }
                }
            }
        }
        if self.dj.decks.any_playing() {
            ctx.request_repaint_after(FRAME);
        } else if self.dj.decks.busy() {
            ctx.request_repaint_after(BUSY_POLL);
        }
    }

    /// Runs one of the decks' commands; what goes wrong is said on the page.
    fn deck_command(&mut self, command: &str, params: Value) -> bool {
        match self.projects.execute(command, &params) {
            Ok(_) => {
                self.dj.decks.owned = true;
                self.dj.decks.error = None;
                // Read again at the next frame, to show what changed.
                self.dj.decks.read_at = None;
                true
            }
            Err(e) => {
                self.dj.decks.error = Some(e.to_string());
                false
            }
        }
    }

    /// Files and folders dropped on the DJ space go into the library.
    pub(in crate::app) fn take_dj_drops(&mut self, dropped: Vec<PathBuf>) {
        let known = self.dj.decks.known();
        let mut files = Vec::new();
        for path in dropped {
            if path.is_dir() {
                files.extend(audio_in(&path));
            } else if is_audio(&path) {
                files.push(path);
            } else {
                let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into());
                self.dj.decks.error = Some(format!("{name} is not a WAV, AIFF, FLAC, MP3 or Ogg file."));
            }
        }
        let files: Vec<_> = files.into_iter().filter(|f| !known.contains(f)).collect();
        if !files.is_empty() {
            self.shell.say(Mood::Joy, format!("{} tracks on their way to your DJ library.", files.len()));
        }
        self.dj.decks.add(files.into_iter().map(|f| (f, None)));
    }

    /// The DJ space: two decks and the mixer, the library below.
    pub(in crate::app) fn show_decks(&mut self, ui: &mut Ui) {
        self.poll_decks(ui.ctx());
        let space = super::menu_bar::WorkspaceMode::Dj;
        ui.add_space(theme::PAD * 2.0);
        self.dj_header(ui, space);
        ui.add_space(GAP);
        let width = ui.available_width();
        let deck_width = ((width - MIXER_WIDTH - GAP * 2.0) / 2.0).max(200.0);
        let height = 300.0;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            let down = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(Vec2::new(deck_width, height), down, |ui| self.deck_card(ui, 0, height));
            ui.allocate_ui_with_layout(Vec2::new(MIXER_WIDTH, height), down, |ui| self.mixer_card(ui, height));
            ui.allocate_ui_with_layout(Vec2::new(deck_width, height), down, |ui| self.deck_card(ui, 1, height));
        });
        ui.add_space(GAP);
        self.library_card(ui);
    }

    /// Piou and the page's name on the left, what brings music in on the right.
    fn dj_header(&mut self, ui: &mut Ui, space: super::menu_bar::WorkspaceMode) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::hover());
            let playing = (0..2).find(|&d| self.dj.decks.playing(d));
            let (mood, beat) = match playing {
                Some(deck) => (Mood::Play, self.dj.decks.bpm(deck).map_or(0.5, |bpm| 60.0 / bpm.max(1.0))),
                None => (self.piou_mood(ui.ctx()), self.beat_seconds()),
            };
            piou::show(ui.ctx(), ui.painter(), rect, mood, space.coat(), beat);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("DJ").size(22.0).strong().color(theme::text_primary()));
                ui.label(
                    egui::RichText::new("Two decks and a mixer, right here. Plug in your controller or use the mouse.")
                        .color(theme::text_hint()),
                );
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(egui::Button::new("Open Mixxx instead").corner_radius(BUTTON_RADIUS))
                    .on_hover_text("Mixxx, the free DJ program shipped with YOBUKO, in its own window")
                    .clicked()
                {
                    self.open_mixxx(ui.ctx());
                }
                if ui
                    .add_enabled(
                        self.session().is_some(),
                        egui::Button::new("Send this project to DJ…").corner_radius(BUTTON_RADIUS),
                    )
                    .on_hover_text("Render the project to audio and add it to your DJ library")
                    .on_disabled_hover_text("Open a project first")
                    .clicked()
                {
                    self.open_export();
                    self.dj.next_export_to_dj = true;
                }
                let music = music_folder();
                if ui
                    .add_enabled(
                        music.is_some(),
                        egui::Button::new(egui::RichText::new("Add my Music folder").strong().color(Color32::WHITE))
                            .fill(space.color())
                            .corner_radius(BUTTON_RADIUS),
                    )
                    .on_hover_text("Bring the audio files of your Music folder into the DJ library")
                    .on_disabled_hover_text("No Music folder found: drop files or a folder on this page instead")
                    .clicked()
                    && let Some(music) = music
                {
                    self.take_dj_drops(vec![music]);
                }
            });
        });
    }

    /// One deck: its track, waveforms, transport and tempo.
    fn deck_card(&mut self, ui: &mut Ui, deck: usize, height: f32) {
        let color = deck_color(deck);
        let name = NAMES[deck];
        egui::Frame::NONE
            .fill(theme::panel_background())
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(CARD_RADIUS)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(height - 28.0);
                let d = self.dj.decks.deck(deck).clone();
                let loaded = d["track"].is_number();
                let loading = d["loading"] == true;
                // Title and tempo.
                ui.horizontal(|ui| {
                    let (badge, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
                    ui.painter().rect_filled(badge, CornerRadius::same(8), color);
                    ui.painter().text(
                        badge.center(),
                        Align2::CENTER_CENTER,
                        name,
                        FontId::proportional(15.0),
                        Color32::WHITE,
                    );
                    let title = if loading {
                        "Loading…".to_owned()
                    } else {
                        d["title"].as_str().unwrap_or("Empty deck: load a track from the library").to_owned()
                    };
                    ui.add(egui::Label::new(egui::RichText::new(title).strong().size(14.0)).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let bpm = self.dj.decks.bpm(deck).map_or_else(|| "—".to_owned(), |b| format!("{b:.1}"));
                        ui.label(egui::RichText::new("BPM").small().color(theme::text_hint()));
                        ui.label(egui::RichText::new(bpm).font(theme::mono_font(18.0)).color(color));
                    });
                });
                ui.add_space(theme::PAD);
                let peaks = self.projects.dj_deck_peaks(deck);
                let position = self.dj.decks.position(deck);
                let length = d["seconds"].as_f64().unwrap_or(0.0);
                self.overview(ui, deck, peaks.as_deref(), position, length);
                ui.add_space(theme::PAD);
                close_wave(ui, deck, peaks.as_deref(), position, self.dj.decks.number(deck, "rate", 1.0));
                ui.add_space(theme::PAD);
                // Times.
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(clock(position)).font(theme::mono_font(13.0)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("−{}", clock(length - position)))
                                .font(theme::mono_font(13.0))
                                .color(theme::text_hint()),
                        );
                    });
                });
                ui.add_space(theme::PAD);
                self.transport(ui, deck, loaded && !loading);
                ui.add_space(theme::PAD);
                self.tempo(ui, deck, loaded);
                if let Some(error) = d["error"].as_str() {
                    ui.colored_label(theme::recording(), error);
                }
            });
    }

    /// The whole track, the part played dimmed; a click or a drag goes there.
    fn overview(&mut self, ui: &mut Ui, deck: usize, peaks: Option<&Peaks>, position: f64, length: f64) {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click_and_drag());
        let label = format!("Deck {} track overview", NAMES[deck]);
        response.widget_info(|| egui::WidgetInfo::slider(peaks.is_some(), position, &label));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(8), theme::window_background());
        let color = deck_color(deck);
        if let Some(peaks) = peaks.filter(|_| length > 0.0) {
            let played = (position / length).clamp(0.0, 1.0);
            draw_wave(&painter, rect.shrink2(Vec2::new(4.0, 3.0)), peaks, 0.0, length, color, Some(position));
            let x = rect.left() + rect.width() * played as f32;
            painter.vline(x, rect.y_range(), Stroke::new(2.0, theme::text_primary()));
            if let Some(pointer) = response.interact_pointer_pos()
                && (response.clicked() || response.drag_stopped())
            {
                let to = f64::from((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0) * length;
                self.seek(deck, to);
            }
        } else {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "No track",
                FontId::proportional(12.0),
                theme::text_hint(),
            );
        }
    }

    fn seek(&mut self, deck: usize, to: f64) {
        let length = self.dj.decks.number(deck, "seconds", 0.0);
        let to = to.clamp(0.0, (length - 0.05).max(0.0));
        if self.deck_command("dj.deck.seek", json!({"deck": NAMES[deck], "from": to})) {
            self.dj.decks.set(Some(deck), "position", json!(to));
        }
    }

    /// Cue, Play / Pause and Sync.
    fn transport(&mut self, ui: &mut Ui, deck: usize, ready: bool) {
        let name = NAMES[deck];
        let color = deck_color(deck);
        let playing = self.dj.decks.playing(deck);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let cue = round_button(ui, &format!("Cue {name}"), "CUE", 52.0, None, ready);
            if cue.clicked() {
                if playing {
                    // Back to the cue point, stopped there.
                    self.deck_command("dj.deck.pause", json!({"deck": name}));
                    self.seek(deck, self.dj.decks.cue[deck]);
                    self.dj.decks.set(Some(deck), "playing", json!(false));
                } else {
                    // Stopped: the cue point is set here.
                    self.dj.decks.cue[deck] = self.dj.decks.position(deck);
                }
            }
            let play_text = if playing { "❚❚" } else { "▶" };
            let play = round_button(ui, &format!("Play deck {name}"), play_text, 52.0, Some(color), ready);
            if play.clicked() {
                if playing {
                    if self.deck_command("dj.deck.pause", json!({"deck": name})) {
                        self.dj.decks.set(Some(deck), "playing", json!(false));
                    }
                } else if self.deck_command("dj.deck.play", json!({"deck": name})) {
                    self.dj.decks.set(Some(deck), "playing", json!(true));
                }
            }
            let other = 1 - deck;
            let can_sync = ready && self.dj.decks.deck(deck)["bpm"].is_number() && self.dj.decks.bpm(other).is_some();
            let sync = round_button(ui, &format!("Sync {name}"), "SYNC", 52.0, None, can_sync)
                .on_hover_text("Play at the other deck's tempo")
                .on_disabled_hover_text("Both decks need a track with a known tempo");
            if sync.clicked()
                && let (Some(own), Some(target)) = (self.dj.decks.deck(deck)["bpm"].as_f64(), self.dj.decks.bpm(other))
            {
                self.set_rate(deck, (target / own.max(1.0)).clamp(0.5, 2.0));
            }
        });
    }

    fn set_rate(&mut self, deck: usize, rate: f64) {
        if self.deck_command("dj.deck.rate", json!({"deck": NAMES[deck], "rate": rate})) {
            self.dj.decks.set(Some(deck), "rate", json!(rate));
        }
    }

    /// The tempo slider, in percent of the track's own speed; a double click goes back to 0.
    fn tempo(&mut self, ui: &mut Ui, deck: usize, loaded: bool) {
        let rate = self.dj.decks.number(deck, "rate", 1.0);
        let mut percent = (rate - 1.0) * 100.0;
        let range = if percent.abs() <= 8.0 {
            8.0
        } else if percent.abs() <= 16.0 {
            16.0
        } else {
            50.0
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Tempo").small().color(theme::text_hint()));
            let label = format!("Tempo {}", NAMES[deck]);
            let slider = egui::Slider::new(&mut percent, -range..=range).show_value(false).trailing_fill(true);
            ui.spacing_mut().slider_width = (ui.available_width() - 70.0).max(60.0);
            let response = ui.add_enabled(loaded, slider).on_hover_text("Double-click: back to the track's own tempo");
            response.widget_info(|| egui::WidgetInfo::slider(loaded, percent, &label));
            ui.label(egui::RichText::new(format!("{percent:+.1} %")).font(theme::mono_font(12.0)));
            if response.double_clicked() {
                self.set_rate(deck, 1.0);
            } else if response.changed() {
                self.set_rate(deck, (1.0 + percent / 100.0).clamp(0.5, 2.0));
            }
        });
    }

    /// The mixer: per deck an EQ, a filter, a headphone cue and a level; the crossfader under.
    fn mixer_card(&mut self, ui: &mut Ui, height: f32) {
        egui::Frame::NONE
            .fill(theme::panel_background())
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(CARD_RADIUS)
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(height - 24.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("MIXER").small().color(theme::text_hint()));
                });
                let strip = ((ui.available_width() - GAP) / 2.0).max(50.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    for deck in 0..2 {
                        ui.allocate_ui_with_layout(
                            Vec2::new(strip, 0.0),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| self.channel(ui, deck),
                        );
                    }
                });
                ui.add_space(theme::PAD);
                let mut crossfade = self.dj.decks.snapshot["performance"]["crossfade"].as_f64().unwrap_or(0.5) as f32;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("A").strong().color(deck_color(0)));
                    let changed = crossfader(ui, &mut crossfade, ui.available_width() - 16.0);
                    ui.label(egui::RichText::new("B").strong().color(deck_color(1)));
                    if changed {
                        self.dj.decks.set(None, "crossfade", json!(crossfade));
                        self.send_mixer();
                    }
                });
            });
    }

    /// One deck's strip of the mixer.
    fn channel(&mut self, ui: &mut Ui, deck: usize) {
        let name = NAMES[deck];
        let color = deck_color(deck);
        ui.spacing_mut().item_spacing.y = 2.0;
        let mut tone_changed = false;
        let mut bands = [0.0f32; 3];
        for (band, key) in bands.iter_mut().zip(["high", "mid", "low"]) {
            *band = knob_of_db(self.dj.decks.number(deck, key, 0.0));
        }
        for (band, title) in bands.iter_mut().zip(["High", "Mid", "Low"]) {
            tone_changed |= dj_knob(ui, &format!("{title} {name}"), title, band, 0.5, color, false);
        }
        let mut filter = (self.dj.decks.number(deck, "filter", 0.0) as f32 + 1.0) / 2.0;
        let filter_changed = dj_knob(ui, &format!("Filter {name}"), "Filter", &mut filter, 0.5, color, true);
        if tone_changed || filter_changed {
            let [high, mid, low] = bands.map(db_of_knob);
            let filter = f64::from(filter) * 2.0 - 1.0;
            let filter = if filter.abs() < 0.03 { 0.0 } else { filter };
            for (key, value) in [("high", high), ("mid", mid), ("low", low), ("filter", filter)] {
                self.dj.decks.set(Some(deck), key, json!(value));
            }
            self.deck_command(
                "dj.deck.tone",
                json!({"deck": name, "low": low, "mid": mid, "high": high, "filter": filter}),
            );
        }
        ui.add_space(4.0);
        let separate = self.dj.decks.snapshot["performance"]["separate_cue"] == true;
        let key = if deck == 0 { "cueA" } else { "cueB" };
        let mut cued = self.dj.decks.snapshot["performance"][key] == true;
        let headphones = ui
            .add_enabled(
                separate,
                egui::Button::selectable(cued, egui::RichText::new("Phones").size(11.5)).corner_radius(BUTTON_RADIUS),
            )
            .on_hover_text(format!("Hear deck {name} in the headphones"))
            .on_disabled_hover_text("Choose a separate headphone output first: Options › Settings › Audio, cue output");
        headphones.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, separate, cued, format!("Headphones {name}"))
        });
        if headphones.clicked() {
            cued = !cued;
            self.dj.decks.set(None, key, json!(cued));
            self.send_mixer();
        }
        ui.add_space(4.0);
        let level_key = if deck == 0 { "levelA" } else { "levelB" };
        let mut level = self.dj.decks.snapshot["performance"][level_key].as_f64().unwrap_or(1.0) as f32;
        if fader(ui, &format!("Level {name}"), &mut level, color) {
            self.dj.decks.set(None, level_key, json!(level));
            self.send_mixer();
        }
    }

    fn send_mixer(&mut self) {
        let p = &self.dj.decks.snapshot["performance"];
        let params = json!({
            "crossfade": p["crossfade"].as_f64().unwrap_or(0.5),
            "levelA": p["levelA"].as_f64().unwrap_or(1.0),
            "levelB": p["levelB"].as_f64().unwrap_or(1.0),
            "cueA": p["cueA"].as_bool().unwrap_or(false),
            "cueB": p["cueB"].as_bool().unwrap_or(false),
        });
        self.deck_command("dj.mixer.set", params);
    }

    /// The DJ library: what is in it, Load A / Load B, and how to bring music in.
    fn library_card(&mut self, ui: &mut Ui) {
        egui::Frame::NONE
            .fill(theme::panel_background())
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(CARD_RADIUS)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height().max(160.0) - 28.0);
                let tracks: Vec<Value> = self.dj.decks.snapshot["tracks"].as_array().cloned().unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("LIBRARY").small().color(theme::text_hint()));
                    ui.label(egui::RichText::new(format!("{} tracks", tracks.len())).small().color(theme::text_hint()));
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
                if tracks.is_empty() {
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
                let row = 28.0;
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
            });
    }

    fn library_row(&mut self, ui: &mut Ui, track: &Value, index: usize, height: f32) {
        let id = track["id"].as_u64().unwrap_or(0);
        let title = track["title"].as_str().unwrap_or("Untitled");
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
        if index % 2 == 1 {
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
            if load.clicked() && self.deck_command("dj.deck.load", json!({"deck": name, "track": id})) {
                self.dj.decks.cue[deck] = 0.0;
                self.dj.decks.set(Some(deck), "loading", json!(true));
            }
        }
    }
}

/// A deck's close waveform: [`CLOSE_SECONDS`] around the playhead, which stays in the middle.
fn close_wave(ui: &mut Ui, deck: usize, peaks: Option<&Peaks>, position: f64, rate: f64) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 96.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(10), theme::window_background());
    if let Some(peaks) = peaks {
        let half = CLOSE_SECONDS / 2.0 * rate.clamp(0.5, 2.0);
        draw_wave(
            &painter,
            rect.shrink2(Vec2::new(0.0, 6.0)),
            peaks,
            position - half,
            position + half,
            deck_color(deck),
            Some(position),
        );
    }
    painter.vline(rect.center().x, rect.y_range(), Stroke::new(2.0, theme::text_primary()));
}

/// Draws `peaks` from `from` to `to` seconds across `rect`, what is before `played` dimmed.
fn draw_wave(
    painter: &egui::Painter,
    rect: Rect,
    peaks: &Peaks,
    from: f64,
    to: f64,
    color: Color32,
    played: Option<f64>,
) {
    let rate = f64::from(peaks.sample_rate.max(1));
    let columns = rect.width().max(1.0) as usize;
    let span = (to - from) / columns as f64;
    let half = rect.height() / 2.0;
    let dim = color.gamma_multiply(0.35);
    let mut shapes = Vec::with_capacity(columns);
    for column in 0..columns {
        let start = from + span * column as f64;
        let Some(bucket) = peaks.column(Lane::Mixed, start * rate, (start + span) * rate) else { continue };
        let top = bucket.max.clamp(-1.0, 1.0).max(0.02);
        let bottom = bucket.min.clamp(-1.0, 1.0).min(-0.02);
        let x = rect.left() + column as f32 + 0.5;
        let colour = if played.is_some_and(|p| start + span <= p) { dim } else { color };
        shapes.push(egui::Shape::line_segment(
            [Pos2::new(x, rect.center().y - top * half), Pos2::new(x, rect.center().y - bottom * half)],
            Stroke::new(1.0, colour),
        ));
    }
    painter.extend(shapes);
}

/// A round button with `text`, filled with `fill` when given; `label` names it for screen
/// readers and tests.
fn round_button(
    ui: &mut Ui,
    label: &str,
    text: &str,
    size: f32,
    fill: Option<Color32>,
    enabled: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(size), if enabled { Sense::click() } else { Sense::hover() });
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    let hover = response.hovered() && enabled;
    let base = fill.unwrap_or_else(theme::control_background);
    let face = if !enabled {
        base.gamma_multiply(0.45)
    } else if hover {
        crate::kit::motion::mix(base, Color32::WHITE, 0.12)
    } else {
        base
    };
    let painter = ui.painter();
    painter.circle_filled(rect.center() + Vec2::new(0.0, 2.0), size / 2.0, Color32::from_black_alpha(60));
    painter.circle_filled(rect.center(), size / 2.0, face);
    if fill.is_none() {
        painter.circle_stroke(rect.center(), size / 2.0 - 0.5, Stroke::new(1.0, theme::border()));
    }
    let ink = if fill.is_some() { Color32::WHITE } else { theme::text_primary() };
    let ink = if enabled { ink } else { ink.gamma_multiply(0.5) };
    let font = if text.chars().count() > 2 { FontId::proportional(11.0) } else { FontId::proportional(18.0) };
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, ink);
    response
}

/// A round knob turned by dragging up or down; a double click puts it back at `rest`.
/// `centred` draws the coloured arc from the middle (a filter), else from `rest` too.
fn dj_knob(ui: &mut Ui, label: &str, title: &str, value: &mut f32, rest: f32, color: Color32, centred: bool) -> bool {
    let size = Vec2::new(46.0, 50.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let before = *value;
    if response.dragged() {
        *value = (*value - response.drag_delta().y * 0.005).clamp(0.0, 1.0);
    }
    if response.double_clicked() {
        *value = rest;
    }
    let shown = *value;
    response.widget_info(|| egui::WidgetInfo::slider(true, f64::from(shown), label));
    let response = response.on_hover_text(format!("{title}: drag up or down, double-click to reset"));
    let painter = ui.painter();
    let center = Pos2::new(rect.center().x, rect.top() + 18.0);
    let radius = 16.0;
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        let steps = 32;
        (0..=steps)
            .map(|i| {
                let t = from + (to - from) * i as f32 / steps as f32;
                let a = (-135.0 + 270.0 * t).to_radians();
                center + Vec2::new(a.sin(), -a.cos()) * radius
            })
            .collect()
    };
    painter.add(egui::Shape::line(arc(0.0, 1.0), Stroke::new(3.0, theme::control_background())));
    let start = if centred { 0.5 } else { rest };
    if (shown - start).abs() > 0.005 {
        painter.add(egui::Shape::line(arc(start.min(shown), start.max(shown)), Stroke::new(3.0, color)));
    }
    let hover = response.hovered() || response.dragged();
    let face = if hover { theme::control_hover() } else { theme::control_background() };
    painter.circle_filled(center, radius - 4.0, face);
    let a = (-135.0 + 270.0 * shown).to_radians();
    let tip = center + Vec2::new(a.sin(), -a.cos()) * (radius - 7.0);
    painter.line_segment([center, tip], Stroke::new(2.5, theme::text_primary()));
    painter.text(
        Pos2::new(rect.center().x, rect.bottom() - 6.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(10.5),
        theme::text_hint(),
    );
    (*value - before).abs() > f32::EPSILON
}

/// A deck's level fader, 0 … 1, dragged or clicked anywhere along it.
fn fader(ui: &mut Ui, label: &str, value: &mut f32, color: Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(36.0, 92.0), Sense::click_and_drag());
    let before = *value;
    let track = rect.shrink2(Vec2::new(0.0, 8.0));
    if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        *value = ((track.bottom() - pointer.y) / track.height()).clamp(0.0, 1.0);
    }
    let shown = *value;
    response.widget_info(|| egui::WidgetInfo::slider(true, f64::from(shown), label));
    let painter = ui.painter();
    let line = Rect::from_center_size(track.center(), Vec2::new(6.0, track.height()));
    painter.rect_filled(line, CornerRadius::same(3), theme::control_background());
    let y = track.bottom() - track.height() * shown;
    painter.rect_filled(Rect::from_min_max(Pos2::new(line.left(), y), line.max), CornerRadius::same(3), color);
    let cap = Rect::from_center_size(Pos2::new(track.center().x, y), Vec2::new(30.0, 14.0));
    painter.rect_filled(cap.translate(Vec2::new(0.0, 2.0)), CornerRadius::same(7), Color32::from_black_alpha(70));
    painter.rect_filled(cap, CornerRadius::same(7), theme::text_value());
    painter.rect_stroke(cap, CornerRadius::same(7), Stroke::new(1.0, theme::border()), StrokeKind::Inside);
    painter.hline(cap.shrink2(Vec2::new(7.0, 0.0)).x_range(), cap.center().y, Stroke::new(2.0, color));
    (*value - before).abs() > f32::EPSILON
}

/// The crossfader, A at the left, B at the right; a double click puts it in the middle.
fn crossfader(ui: &mut Ui, value: &mut f32, width: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width.max(80.0), 30.0), Sense::click_and_drag());
    let before = *value;
    let track = rect.shrink2(Vec2::new(12.0, 0.0));
    if response.double_clicked() {
        *value = 0.5;
    } else if let Some(pointer) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        *value = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0);
    }
    let shown = *value;
    response.widget_info(|| egui::WidgetInfo::slider(true, f64::from(shown), "Crossfader"));
    let painter = ui.painter();
    let line = Rect::from_center_size(track.center(), Vec2::new(track.width(), 6.0));
    painter.rect_filled(line, CornerRadius::same(3), theme::control_background());
    painter.vline(
        track.center().x,
        Rect::from_center_size(track.center(), Vec2::new(1.0, 14.0)).y_range(),
        Stroke::new(1.0, theme::text_hint()),
    );
    let x = track.left() + track.width() * shown;
    let cap = Rect::from_center_size(Pos2::new(x, track.center().y), Vec2::new(18.0, 26.0));
    painter.rect_filled(cap.translate(Vec2::new(0.0, 2.0)), CornerRadius::same(7), Color32::from_black_alpha(70));
    painter.rect_filled(cap, CornerRadius::same(7), theme::text_value());
    let mix = crate::kit::motion::mix(deck_color(0), deck_color(1), shown);
    painter.vline(cap.center().x, cap.shrink2(Vec2::new(0.0, 6.0)).y_range(), Stroke::new(2.0, mix));
    (*value - before).abs() > f32::EPSILON
}

#[cfg(test)]
#[path = "dj_decks_tests.rs"]
mod tests;
