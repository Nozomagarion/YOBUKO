// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ space's own mixer, drawn in YOBUKO's window: two decks (A and B) with their
//! waveforms, Play, Cue, Sync, nudge, tempo, hot cues, loops and beat jumps (`dj_deck_tools.rs`);
//! a mixer with a trim, a three-band EQ, a filter, a level, a meter and a headphone cue per deck,
//! the master and headphone levels and a crossfader; and the DJ library below to search, sort and
//! load them from, with its playlists (`dj_library.rs`). It drives the session's `dj.*`
//! commands; the sound is the engine's two decks (`suhina_engine::decks`), on the same audio
//! output as the studio.

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
const MIXER_WIDTH: f32 = 300.0;
/// The decks' and the mixer's height.
const DECK_HEIGHT: f32 = 440.0;
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
    /// The library's row a controller's Load buttons load, moved by its browse knob or a click.
    selected: usize,
    /// The DJ controllers whose ports the decks have taken, by name, and what they send.
    controller: Option<(Vec<String>, std::sync::mpsc::Receiver<suhina_session::ControllerMessage>)>,
    /// Each deck's loops, nudge, tempo range and meter, kept by the page.
    tools: [tools::DeckTools; 2],
    /// The master meter's needles.
    master_meter: [f32; 2],
    /// The controller's SHIFT buttons, held.
    shift: [bool; 2],
    /// Whether each deck quantizes: cues and loops land on the beat, jumps keep the beat.
    quantize: [bool; 2],
    /// Whether the effect units show; `None` until chosen, when they show if there is room.
    show_fx: Option<bool>,
    /// Whether they showed last frame.
    fx_shown: bool,
    /// The same for the sampler's row.
    show_sampler: Option<bool>,
    sampler_shown: bool,
    /// Whether each deck's pads play the sampler's slots instead of its hot cues.
    sampler_pads: [bool; 2],
    /// The library's search, sort and playlist.
    pub(in crate::app) library: library::LibraryView,
}

impl Decks {
    /// Moves the meters' needles to the peaks just read, falling back slowly from the last.
    fn read_meters(&mut self) {
        let fall = |shown: f32, peak: f32| peak.max(shown * 0.6);
        for deck in 0..2 {
            let peaks = &self.snapshot["performance"]["decks"][deck]["meter"];
            for side in 0..2 {
                let peak = peaks[side].as_f64().unwrap_or(0.0) as f32;
                self.tools[deck].meter[side] = fall(self.tools[deck].meter[side], peak);
            }
        }
        for side in 0..2 {
            let peak = self.snapshot["performance"]["masterMeter"][side].as_f64().unwrap_or(0.0) as f32;
            self.master_meter[side] = fall(self.master_meter[side], peak);
        }
    }

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

    /// The track's own tempo: its beat grid's, else the analysed one.
    fn track_bpm(&self, deck: usize) -> Option<f64> {
        self.beat(deck).map(|(_, beat)| 60.0 / beat)
    }

    /// The deck's tempo as it plays: the track's BPM times its speed.
    fn bpm(&self, deck: usize) -> Option<f64> {
        self.track_bpm(deck).map(|bpm| bpm * self.number(deck, "rate", 1.0))
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

/// The trim knob's place (0 … 1, the middle 0 dB) for its dB, and back: ± 12 dB evenly.
fn trim_knob(db: f64) -> f32 {
    let range = f64::from(suhina_engine::DECK_TRIM_DB);
    (0.5 + 0.5 * db.clamp(-range, range) / range) as f32
}

fn trim_db(knob: f32) -> f64 {
    let db = f64::from(suhina_engine::DECK_TRIM_DB) * (f64::from(knob.clamp(0.0, 1.0)) - 0.5) / 0.5;
    if db.abs() < 0.3 { 0.0 } else { db }
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
                // A command just ran: read what it changed.
                (_, None) => true,
                (None, Some(_)) => false,
            };
        if due {
            match self.projects.execute("dj.library", &json!({})) {
                Ok(snapshot) => {
                    if let Some(error) = snapshot["error"].as_str() {
                        self.dj.decks.error = Some(error.to_owned());
                    }
                    self.dj.decks.snapshot = snapshot;
                    self.dj.decks.read_at = Some(Instant::now());
                    self.dj.decks.read_meters();
                    if !self.dj.decks.any_playing() {
                        // Nothing plays: the meters drop.
                        self.dj.decks.tools.iter_mut().for_each(|t| t.meter = [0.0; 2]);
                        self.dj.decks.master_meter = [0.0; 2];
                    }
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
        let deck_width = ((width - MIXER_WIDTH - GAP * 2.0) / 2.0).max(300.0);
        let height = DECK_HEIGHT;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            let down = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(Vec2::new(deck_width, height), down, |ui| self.deck_card(ui, 0, height));
            ui.allocate_ui_with_layout(Vec2::new(MIXER_WIDTH, height), down, |ui| self.mixer_card(ui, height));
            ui.allocate_ui_with_layout(Vec2::new(deck_width, height), down, |ui| self.deck_card(ui, 1, height));
        });
        ui.add_space(GAP);
        // The effects show when there is room for them and a few library rows, or when asked.
        let room = ui.available_height() > fx::FX_HEIGHT + GAP + 260.0;
        self.dj.decks.fx_shown = self.dj.decks.show_fx.unwrap_or(room);
        if self.dj.decks.fx_shown {
            self.fx_row(ui);
            ui.add_space(GAP);
        }
        let room = ui.available_height() > sampler::SAMPLER_HEIGHT + GAP + 260.0;
        self.dj.decks.sampler_shown = self.dj.decks.show_sampler.unwrap_or(room);
        if self.dj.decks.sampler_shown {
            self.sampler_row(ui);
            ui.add_space(GAP);
        }
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
                let shown = self.dj.decks.show_sampler.unwrap_or(self.dj.decks.sampler_shown);
                if ui
                    .add(egui::Button::selectable(shown, "Sampler").corner_radius(BUTTON_RADIUS))
                    .on_hover_text("Show or hide the sampler's eight slots")
                    .clicked()
                {
                    self.dj.decks.show_sampler = Some(!shown);
                }
                let shown = self.dj.decks.show_fx.unwrap_or(self.dj.decks.fx_shown);
                if ui
                    .add(egui::Button::selectable(shown, "Effects").corner_radius(BUTTON_RADIUS))
                    .on_hover_text("Show or hide the two effect units")
                    .clicked()
                {
                    self.dj.decks.show_fx = Some(!shown);
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

    /// One deck: its track, waveforms, transport, hot cues, loops and tempo.
    fn deck_card(&mut self, ui: &mut Ui, deck: usize, height: f32) {
        let color = deck_color(deck);
        let name = NAMES[deck];
        // A nudge button held this frame says so while it is drawn.
        self.dj.decks.tools[deck].held = 0.0;
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
                let ready = loaded && !loading;
                // Title, key, tempo and eject.
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if tools::small_button(ui, &format!("Eject {name}"), "EJECT", false, color, ready)
                            .on_hover_text("Take the track out of the deck")
                            .clicked()
                        {
                            self.eject(deck);
                        }
                        let bpm = self.dj.decks.bpm(deck).map_or_else(|| "—".to_owned(), |b| format!("{b:.1}"));
                        ui.label(egui::RichText::new("BPM").small().color(theme::text_hint()));
                        ui.label(egui::RichText::new(bpm).font(theme::mono_font(18.0)).color(color));
                        if let Some(key) = d["key"].as_str().filter(|k| !k.is_empty()) {
                            ui.label(egui::RichText::new(key).font(theme::mono_font(13.0)).color(theme::text_hint()));
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(egui::RichText::new(title).strong().size(14.0)).truncate());
                        });
                    });
                });
                ui.add_space(theme::PAD);
                let peaks = self.projects.dj_deck_peaks(deck);
                let position = self.dj.decks.position(deck);
                let length = d["seconds"].as_f64().unwrap_or(0.0);
                let marks = self.marks(deck);
                self.overview(ui, deck, peaks.as_deref(), position, length, &marks);
                ui.add_space(theme::PAD);
                close_wave(ui, deck, peaks.as_deref(), position, self.dj.decks.number(deck, "rate", 1.0), &marks);
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
                self.transport(ui, deck, ready);
                ui.add_space(theme::PAD);
                if self.dj.decks.sampler_pads[deck] {
                    self.sampler_pads(ui, deck);
                } else {
                    self.pads(ui, deck, ready);
                }
                ui.add_space(6.0);
                self.loop_row(ui, deck, ready);
                ui.add_space(6.0);
                self.key_row(ui, deck, ready);
                ui.add_space(theme::PAD);
                self.tempo(ui, deck, loaded);
                if let Some(error) = d["error"].as_str() {
                    ui.colored_label(theme::recording(), error);
                }
            });
        self.apply_bend(deck);
    }

    /// What the waveforms mark: the hot cues, the loop and the cue point.
    fn marks(&self, deck: usize) -> Marks {
        let cues = self
            .dj
            .decks
            .hot_cues(deck)
            .iter()
            .enumerate()
            .filter_map(|(pad, cue)| cue.map(|(_, seconds)| (seconds, tools::PAD_COLORS[pad])))
            .collect();
        Marks {
            cues,
            looped: self.dj.decks.current_loop(deck),
            loop_in: self.dj.decks.tools[deck].loop_in,
            cue: self.dj.decks.track(deck).map(|_| self.dj.decks.cue[deck]),
            beat: self.dj.decks.beat(deck),
            colors: self.projects.dj_deck_colors(deck),
        }
    }

    /// The whole track, the part played dimmed; a click or a drag goes there.
    fn overview(&mut self, ui: &mut Ui, deck: usize, peaks: Option<&Peaks>, position: f64, length: f64, marks: &Marks) {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click_and_drag());
        let label = format!("Deck {} track overview", NAMES[deck]);
        response.widget_info(|| egui::WidgetInfo::slider(peaks.is_some(), position, &label));
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, CornerRadius::same(8), theme::window_background());
        let color = deck_color(deck);
        if let Some(peaks) = peaks.filter(|_| length > 0.0) {
            let inner = rect.shrink2(Vec2::new(4.0, 3.0));
            marks.paint(&painter, inner, 0.0, length);
            let played = (position / length).clamp(0.0, 1.0);
            draw_wave(&painter, inner, peaks, marks.colors.as_deref(), (0.0, length), color, Some(position));
            let x = inner.left() + inner.width() * played as f32;
            painter.vline(x, rect.y_range(), Stroke::new(2.0, theme::text_primary()));
            if let Some(pointer) = response.interact_pointer_pos()
                && (response.clicked() || response.drag_stopped())
            {
                let to = f64::from((pointer.x - inner.left()) / inner.width()).clamp(0.0, 1.0) * length;
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

    /// Cue, Play / Pause and Sync; the nudge buttons and back to the start.
    fn transport(&mut self, ui: &mut Ui, deck: usize, ready: bool) {
        let name = NAMES[deck];
        let color = deck_color(deck);
        let playing = self.dj.decks.playing(deck);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let cue = round_button(ui, &format!("Cue {name}"), "CUE", 52.0, None, ready);
            if cue.clicked() {
                self.press_cue(deck);
            }
            let play_text = if playing { "❚❚" } else { "▶" };
            let play = round_button(ui, &format!("Play deck {name}"), play_text, 52.0, Some(color), ready);
            if play.clicked() {
                self.toggle_play(deck);
            }
            let other = 1 - deck;
            let can_sync = ready && self.dj.decks.track_bpm(deck).is_some() && self.dj.decks.bpm(other).is_some();
            let sync = round_button(ui, &format!("Sync {name}"), "SYNC", 52.0, None, can_sync)
                .on_hover_text("Play at the other deck's tempo, with the beats lined up")
                .on_disabled_hover_text("Both decks need a track with a known tempo");
            if sync.clicked() {
                self.sync_deck(deck);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let faster = round_button(ui, &format!("Nudge faster {name}"), "+", 34.0, None, ready && playing)
                    .on_hover_text("Hold: a little faster, to bring the beats together");
                let slower = round_button(ui, &format!("Nudge slower {name}"), "−", 34.0, None, ready && playing)
                    .on_hover_text("Hold: a little slower, to bring the beats together");
                if faster.is_pointer_button_down_on() {
                    self.dj.decks.tools[deck].held = tools::NUDGE;
                } else if slower.is_pointer_button_down_on() {
                    self.dj.decks.tools[deck].held = -tools::NUDGE;
                }
                if round_button(ui, &format!("Back to start {name}"), "|◀", 34.0, None, ready)
                    .on_hover_text("Back to the start of the track")
                    .clicked()
                {
                    self.back_to_start(deck);
                }
                self.pad_mode(ui, deck);
            });
        });
    }

    /// Play, or pause when playing.
    fn toggle_play(&mut self, deck: usize) {
        let name = NAMES[deck];
        if self.dj.decks.playing(deck) {
            if self.deck_command("dj.deck.pause", json!({"deck": name})) {
                self.dj.decks.set(Some(deck), "playing", json!(false));
            }
        } else if self.dj.decks.deck(deck)["track"].is_number()
            && self.deck_command("dj.deck.play", json!({"deck": name}))
        {
            self.dj.decks.set(Some(deck), "playing", json!(true));
        }
    }

    /// Cue: while playing, back to the cue point and stopped there; stopped, the cue point is
    /// set where the deck is.
    fn press_cue(&mut self, deck: usize) {
        if self.dj.decks.playing(deck) {
            self.deck_command("dj.deck.pause", json!({"deck": NAMES[deck]}));
            self.seek(deck, self.dj.decks.cue[deck]);
            self.dj.decks.set(Some(deck), "playing", json!(false));
        } else {
            self.dj.decks.cue[deck] = self.dj.decks.quantized(deck, self.dj.decks.position(deck));
        }
    }

    /// Sync: the deck plays at the other deck's tempo, its beats lined up with the other's
    /// when both play.
    fn sync_deck(&mut self, deck: usize) {
        if let (Some(own), Some(target)) = (self.dj.decks.track_bpm(deck), self.dj.decks.bpm(1 - deck)) {
            self.set_rate(deck, (target / own.max(1.0)).clamp(0.5, 2.0));
            self.align_beats(deck);
        }
    }

    /// The deck in the headphones, or out of them.
    fn toggle_phones(&mut self, deck: usize) {
        let key = if deck == 0 { "cueA" } else { "cueB" };
        let cued = self.dj.decks.snapshot["performance"][key] == true;
        self.dj.decks.set(None, key, json!(!cued));
        self.send_mixer();
    }

    /// Sends the deck's EQ, filter and trim as the page shows them.
    fn send_tone(&mut self, deck: usize) {
        let d = self.dj.decks.deck(deck);
        let params = json!({
            "deck": NAMES[deck],
            "low": d["low"].as_f64().unwrap_or(0.0),
            "mid": d["mid"].as_f64().unwrap_or(0.0),
            "high": d["high"].as_f64().unwrap_or(0.0),
            "filter": d["filter"].as_f64().unwrap_or(0.0),
            "trim": d["trim"].as_f64().unwrap_or(0.0),
        });
        self.deck_command("dj.deck.tone", params);
    }

    /// Loads the library's selected track on `deck`, unless it plays.
    fn load_selected(&mut self, deck: usize) {
        let shown = self.dj.decks.shown_tracks();
        let Some(id) = shown.get(self.dj.decks.selected).and_then(|t| t["id"].as_u64()) else { return };
        if !self.dj.decks.playing(deck) && self.deck_command("dj.deck.load", json!({"deck": NAMES[deck], "track": id}))
        {
            self.dj.decks.cue[deck] = 0.0;
            self.dj.decks.set(Some(deck), "loading", json!(true));
        }
    }

    fn set_rate(&mut self, deck: usize, rate: f64) {
        if self.deck_command("dj.deck.rate", json!({"deck": NAMES[deck], "rate": rate})) {
            self.dj.decks.set(Some(deck), "rate", json!(rate));
        }
    }

    /// The tempo slider, in percent of the track's own speed; a double click goes back to 0.
    /// Its range (±8, ±16 or ±50 %) changes with a click on the range.
    fn tempo(&mut self, ui: &mut Ui, deck: usize, loaded: bool) {
        let rate = self.dj.decks.number(deck, "rate", 1.0);
        let mut percent = (rate - 1.0) * 100.0;
        let tools = &mut self.dj.decks.tools[deck];
        // A tempo beyond the range (a sync, the controller) widens it.
        while percent.abs() > tools::TEMPO_RANGES[tools.range] + 1e-9 && tools.range + 1 < tools::TEMPO_RANGES.len() {
            tools.range += 1;
        }
        let range = tools::TEMPO_RANGES[tools.range];
        let name = NAMES[deck];
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Tempo").small().color(theme::text_hint()));
            let label = format!("Tempo {name}");
            let slider = egui::Slider::new(&mut percent, -range..=range).show_value(false).trailing_fill(true);
            ui.spacing_mut().slider_width = (ui.available_width() - 130.0).max(60.0);
            let response = ui.add_enabled(loaded, slider).on_hover_text("Double-click: back to the track's own tempo");
            response.widget_info(|| egui::WidgetInfo::slider(loaded, percent, &label));
            ui.label(egui::RichText::new(format!("{percent:+.1} %")).font(theme::mono_font(12.0)));
            let shown = format!("±{range:.0} %");
            if tools::small_button(ui, &format!("Tempo range {name}"), &shown, false, deck_color(deck), true)
                .on_hover_text("How far the tempo fader goes")
                .clicked()
            {
                let tools = &mut self.dj.decks.tools[deck];
                let next = (tools.range + 1) % tools::TEMPO_RANGES.len();
                // A narrower range than the tempo brings the tempo inside it.
                tools.range = next;
                let limit = tools::TEMPO_RANGES[next];
                if percent.abs() > limit {
                    self.set_rate(deck, 1.0 + percent.clamp(-limit, limit) / 100.0);
                }
            }
            if response.double_clicked() {
                self.set_rate(deck, 1.0);
            } else if response.changed() {
                self.set_rate(deck, (1.0 + percent / 100.0).clamp(0.5, 2.0));
            }
        });
    }

    /// The mixer: per deck a trim, an EQ, a filter, a headphone cue, a level and a meter; the
    /// master and headphones in the middle; the crossfader under.
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
                let middle = 64.0;
                let strip = ((ui.available_width() - middle - GAP * 2.0) / 2.0).max(50.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = GAP;
                    let down = egui::Layout::top_down(egui::Align::Center);
                    ui.allocate_ui_with_layout(Vec2::new(strip, 0.0), down, |ui| self.channel(ui, 0));
                    ui.allocate_ui_with_layout(Vec2::new(middle, 0.0), down, |ui| self.master_strip(ui));
                    ui.allocate_ui_with_layout(Vec2::new(strip, 0.0), down, |ui| self.channel(ui, 1));
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
        let mut trim = trim_knob(self.dj.decks.number(deck, "trim", 0.0));
        let trim_changed = dj_knob(ui, &format!("Trim {name}"), "Trim", &mut trim, 0.5, color, true);
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
        if tone_changed || filter_changed || trim_changed {
            let [high, mid, low] = bands.map(db_of_knob);
            let filter = f64::from(filter) * 2.0 - 1.0;
            let filter = if filter.abs() < 0.03 { 0.0 } else { filter };
            let trim = trim_db(trim);
            for (key, value) in [("high", high), ("mid", mid), ("low", low), ("filter", filter), ("trim", trim)] {
                self.dj.decks.set(Some(deck), key, json!(value));
            }
            self.send_tone(deck);
        }
        ui.add_space(4.0);
        let separate = self.dj.decks.snapshot["performance"]["separate_cue"] == true;
        let key = if deck == 0 { "cueA" } else { "cueB" };
        let cued = self.dj.decks.snapshot["performance"][key] == true;
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
            self.toggle_phones(deck);
        }
        ui.add_space(4.0);
        let level_key = if deck == 0 { "levelA" } else { "levelB" };
        let mut level = self.dj.decks.snapshot["performance"][level_key].as_f64().unwrap_or(1.0) as f32;
        let meter_levels = self.dj.decks.tools[deck].meter;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let side = (ui.available_width() - 36.0 - 14.0) / 2.0;
            ui.add_space(side.max(0.0));
            if fader(ui, &format!("Level {name}"), &mut level, color) {
                self.dj.decks.set(None, level_key, json!(level));
                self.send_mixer();
            }
            meter(ui, &format!("Meter {name}"), meter_levels, Vec2::new(10.0, 92.0));
        });
    }

    /// The master level and its meter, the headphones' level and their cue / master blend.
    fn master_strip(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing.y = 2.0;
        let p = &self.dj.decks.snapshot["performance"];
        let mut master = p["master"].as_f64().unwrap_or(1.0) as f32;
        let mut phones = p["phones"].as_f64().unwrap_or(1.0) as f32;
        let mut blend = p["phonesMix"].as_f64().unwrap_or(0.0) as f32;
        let separate = p["separate_cue"] == true;
        let ink = theme::text_primary();
        let mut changed = dj_knob(ui, "Master", "Master", &mut master, 1.0, ink, false);
        ui.add_space(4.0);
        let levels = self.dj.decks.master_meter;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.add_space(((ui.available_width() - 23.0) / 2.0).max(0.0));
            meter(ui, "Master meter", levels, Vec2::new(20.0, 120.0));
        });
        ui.add_space(6.0);
        ui.add_enabled_ui(separate, |ui| {
            changed |= dj_knob(ui, "Headphones level", "Phones", &mut phones, 1.0, ink, false);
            changed |= dj_knob(ui, "Headphones mix", "Cue/Mst", &mut blend, 0.0, ink, false);
        });
        if changed {
            self.dj.decks.set(None, "master", json!(master));
            self.dj.decks.set(None, "phones", json!(phones));
            self.dj.decks.set(None, "phonesMix", json!(blend));
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
            "master": p["master"].as_f64().unwrap_or(1.0),
            "phones": p["phones"].as_f64().unwrap_or(1.0),
            "phonesMix": p["phonesMix"].as_f64().unwrap_or(0.0),
        });
        self.deck_command("dj.mixer.set", params);
    }
}

impl App {
    /// While the DJ space shows, a DDJ-FLX4 drives the decks: its port is taken from the
    /// studio's armed tracks and what it sends is read here. Called every frame.
    pub(in crate::app) fn dj_controller(&mut self, ctx: &egui::Context) {
        let showing = self.dj.active && !self.dj.usb;
        let wanted: Vec<String> = if showing {
            self.projects.midi_ports().iter().filter(|p| super::flx4::is_flx4(p)).cloned().collect()
        } else {
            Vec::new()
        };
        let current = self.dj.decks.controller.as_ref().map(|(ports, _)| ports.clone()).unwrap_or_default();
        if wanted != current {
            let input = self.projects.live_input();
            if wanted.is_empty() {
                input.release();
                self.dj.decks.controller = None;
            } else {
                let ids = wanted.iter().map(|p| suhina_engine::port_id(p)).collect();
                self.dj.decks.controller = Some((wanted, input.capture(ids)));
                self.shell.say(Mood::Joy, "Your DDJ-FLX4 drives the decks.");
            }
        }
        let Some((_, messages)) = self.dj.decks.controller.as_ref() else { return };
        let read: Vec<_> = messages.try_iter().take(256).filter_map(|(_, m)| super::flx4::read(m)).collect();
        for message in read {
            self.flx4(message);
        }
        // A jog wheel's nudge ends a moment after its last turn.
        for deck in 0..2 {
            if self.dj.decks.tools[deck].jog.is_some() {
                self.apply_bend(deck);
            }
        }
        // The controller is read at the next frames even when nothing else moves.
        ctx.request_repaint_after(FRAME);
    }

    fn flx4(&mut self, message: super::flx4::Flx4) {
        use super::flx4::Flx4;
        match message {
            Flx4::Play(deck) => self.toggle_play(deck),
            Flx4::Cue(deck) => self.press_cue(deck),
            Flx4::Sync(deck) => self.sync_deck(deck),
            Flx4::Phones(deck) => {
                if self.dj.decks.snapshot["performance"]["separate_cue"] == true {
                    self.toggle_phones(deck);
                }
            }
            Flx4::Load(deck) => self.load_selected(deck),
            Flx4::Browse(steps) => {
                let tracks = self.dj.decks.shown_tracks().len();
                let to = i64::try_from(self.dj.decks.selected).unwrap_or(0) + i64::from(steps);
                self.dj.decks.selected =
                    usize::try_from(to.clamp(0, i64::try_from(tracks).unwrap_or(0) - 1).max(0)).unwrap_or(0);
            }
            Flx4::Level(deck, level) => {
                self.dj.decks.set(None, if deck == 0 { "levelA" } else { "levelB" }, json!(level));
                self.send_mixer();
            }
            Flx4::Crossfader(position) => {
                self.dj.decks.set(None, "crossfade", json!(position));
                self.send_mixer();
            }
            Flx4::Eq(deck, band, knob) => {
                // The knob's middle detent sends 64, a hair past the middle: flat.
                let knob = if (knob - 0.5).abs() < 0.01 { 0.5 } else { knob };
                let key = ["low", "mid", "high"][band.min(2)];
                self.dj.decks.set(Some(deck), key, json!(db_of_knob(knob)));
                self.send_tone(deck);
            }
            Flx4::Filter(deck, knob) => {
                let filter = f64::from(knob) * 2.0 - 1.0;
                let filter = if filter.abs() < 0.03 { 0.0 } else { filter.clamp(-1.0, 1.0) };
                self.dj.decks.set(Some(deck), "filter", json!(filter));
                self.send_tone(deck);
            }
            Flx4::Tempo(deck, fader) => {
                // ±8 % over the fader's length, the way the controller's own mapping reads it.
                let percent = 8.0 * (1.0 - 2.0 * f64::from(fader));
                if self.dj.decks.deck(deck)["track"].is_number() {
                    self.set_rate(deck, 1.0 + percent / 100.0);
                }
            }
            Flx4::Jog(deck, steps) => {
                if self.dj.decks.deck(deck)["track"].is_number() && steps != 0 {
                    if self.dj.decks.playing(deck) {
                        // Playing, the wheel's turn nudges the deck faster or slower.
                        self.jog_nudge(deck, steps);
                    } else {
                        // Stopped, it scrubs.
                        self.seek(deck, self.dj.decks.position(deck) + f64::from(steps) * 0.02);
                    }
                }
            }
            Flx4::Shift(deck, held) => self.dj.decks.shift[deck] = held,
            Flx4::Tap(deck) => self.tap(deck),
            Flx4::ToStart(deck) => self.back_to_start(deck),
            Flx4::Trim(deck, knob) => {
                let knob = if (knob - 0.5).abs() < 0.01 { 0.5 } else { knob };
                self.dj.decks.set(Some(deck), "trim", json!(trim_db(knob)));
                self.send_tone(deck);
            }
            Flx4::PhonesMix(knob) => {
                self.dj.decks.set(None, "phonesMix", json!(knob));
                self.send_mixer();
            }
            Flx4::LoopIn(deck) => self.loop_in(deck),
            Flx4::LoopOut(deck) => self.loop_out(deck),
            Flx4::ReloopExit(deck) => self.reloop(deck),
            Flx4::LoopCall(deck, longer) => self.scale_loop(deck, if longer { 2.0 } else { 0.5 }),
            Flx4::HotCue(deck, pad, delete) => {
                if delete || self.dj.decks.shift[deck] {
                    self.delete_hot_cue(deck, pad);
                } else {
                    self.hot_cue(deck, pad);
                }
            }
            Flx4::BeatLoop(deck, beats) => self.beat_loop(deck, beats),
            Flx4::BeatJump(deck, beats) => self.beat_jump(deck, beats),
            // BEAT FX drives effect unit 1's first effect.
            Flx4::Sampler(slot, stop) => {
                if stop {
                    self.stop_sampler(slot);
                } else {
                    self.sampler_pad(slot);
                }
            }
            Flx4::FxSelect(by) => self.step_fx(0, 0, by),
            Flx4::FxBeats(by) => self.step_fx_beats(0, by),
            Flx4::FxOn => self.toggle_fx(0, 0),
            Flx4::FxAllOff => {
                for slot in 0..3 {
                    self.set_fx(0, json!({"slot": slot + 1, "on": false}));
                }
            }
            Flx4::FxChannel(deck, on) => {
                let other = self.dj.decks.fx(0)[if deck == 0 { "deckB" } else { "deckA" }] == true;
                let mut targets = json!({"deckA": false, "deckB": false, "master": false});
                if on {
                    targets[if deck == 0 { "deckA" } else { "deckB" }] = json!(true);
                } else if !other {
                    targets["master"] = json!(true);
                } else {
                    targets[if deck == 0 { "deckB" } else { "deckA" }] = json!(true);
                }
                self.set_fx(0, targets);
            }
            Flx4::FxLevel(knob) => {
                // LEVEL/DEPTH is the unit's mix; with SHIFT, the effect's own knob.
                if self.dj.decks.shift.iter().any(|s| *s) {
                    self.set_fx(0, json!({"slot": 1, "meta": f64::from(knob)}));
                } else {
                    self.set_fx(0, json!({"mix": f64::from(knob)}));
                }
            }
        }
    }
}

/// What a deck's waveforms mark besides the sound.
struct Marks {
    /// The hot cues, in seconds, in their pads' colours.
    cues: Vec<(f64, Color32)>,
    /// The loop: from, to, and whether it plays.
    looped: Option<(f64, f64, bool)>,
    /// Where LOOP IN was pressed, waiting for LOOP OUT.
    loop_in: Option<f64>,
    /// The cue point the CUE button goes back to.
    cue: Option<f64>,
    /// The beat grid: the first beat and one beat's length, in seconds.
    beat: Option<(f64, f64)>,
    /// The waveform's colours: lows, mids and highs over time.
    colors: Option<std::sync::Arc<suhina_session::WaveColors>>,
}

impl Marks {
    /// Paints the marks across `rect`, which shows `from` to `to` seconds; the loop under the
    /// waveform, the cues over it.
    fn paint(&self, painter: &egui::Painter, rect: Rect, from: f64, to: f64) {
        let span = (to - from).max(1e-6);
        let x = |seconds: f64| rect.left() + rect.width() * ((seconds - from) / span) as f32;
        let inside = |seconds: f64| seconds >= from && seconds <= to;
        if let Some((a, b, on)) = self.looped {
            let left = x(a).max(rect.left());
            let right = x(b).min(rect.right());
            if right > left {
                let fill = if on {
                    Color32::from_rgba_unmultiplied(0x22, 0xC5, 0x5E, 60)
                } else {
                    Color32::from_white_alpha(18)
                };
                painter.rect_filled(Rect::from_x_y_ranges(left..=right, rect.y_range()), CornerRadius::same(4), fill);
            }
        }
        // The beat grid, where it is not too dense to read: every beat, the bars brighter.
        if let Some((first, beat)) = self.beat
            && f64::from(rect.width()) / (span / beat) >= 6.0
        {
            let start = ((from - first) / beat).ceil() as i64;
            let end = ((to - first) / beat).floor() as i64;
            for n in start..=end.min(start + 4096) {
                let line =
                    if n.rem_euclid(4) == 0 { Color32::from_white_alpha(70) } else { Color32::from_white_alpha(26) };
                painter.vline(x(first + n as f64 * beat), rect.y_range(), Stroke::new(1.0, line));
            }
        }
        let stroke = |color: Color32| Stroke::new(1.5, color);
        if let Some(seconds) = self.loop_in.filter(|s| inside(*s)) {
            painter.vline(x(seconds), rect.y_range(), stroke(Color32::from_rgb(0x22, 0xC5, 0x5E)));
        }
        if let Some(seconds) = self.cue.filter(|s| inside(*s)) {
            let top = Pos2::new(x(seconds), rect.top());
            painter.add(egui::Shape::convex_polygon(
                vec![top + Vec2::new(-5.0, 0.0), top + Vec2::new(5.0, 0.0), top + Vec2::new(0.0, 6.0)],
                Color32::from_rgb(0xF5, 0x9E, 0x0B),
                Stroke::NONE,
            ));
        }
        for &(seconds, color) in self.cues.iter().filter(|(s, _)| inside(*s)) {
            let at = x(seconds);
            painter.vline(at, rect.y_range(), stroke(color));
            let bottom = Pos2::new(at, rect.bottom());
            painter.add(egui::Shape::convex_polygon(
                vec![bottom + Vec2::new(-5.0, 0.0), bottom + Vec2::new(0.0, -6.0), bottom + Vec2::new(5.0, 0.0)],
                color,
                Stroke::NONE,
            ));
        }
    }
}

/// A deck's close waveform: [`CLOSE_SECONDS`] around the playhead, which stays in the middle.
fn close_wave(ui: &mut Ui, deck: usize, peaks: Option<&Peaks>, position: f64, rate: f64, marks: &Marks) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 96.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(10), theme::window_background());
    if let Some(peaks) = peaks {
        let half = CLOSE_SECONDS / 2.0 * rate.clamp(0.5, 2.0);
        let inner = rect.shrink2(Vec2::new(0.0, 6.0));
        marks.paint(&painter, inner, position - half, position + half);
        let shown = (position - half, position + half);
        draw_wave(&painter, inner, peaks, marks.colors.as_deref(), shown, deck_color(deck), Some(position));
    }
    painter.vline(rect.center().x, rect.y_range(), Stroke::new(2.0, theme::text_primary()));
}

/// A level meter, one bar per side: green, then yellow from −6 dB, red from 0 dB.
fn meter(ui: &mut Ui, label: &str, levels: [f32; 2], size: Vec2) {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let db = |v: f32| if v > 1e-6 { 20.0 * v.log10() } else { -96.0 };
    let loudest = db(levels[0].max(levels[1]));
    response.widget_info(|| egui::WidgetInfo::slider(false, f64::from(loudest.max(-96.0)), label));
    let painter = ui.painter();
    let bars = if size.x >= 16.0 { 2 } else { 1 };
    let gap = 2.0;
    let width = (rect.width() - gap * (bars as f32 - 1.0)) / bars as f32;
    // −48 dB at the bottom, +6 dB at the top.
    let place = |d: f32| ((d + 48.0) / 54.0).clamp(0.0, 1.0);
    for bar in 0..bars {
        let level = if bars == 1 { levels[0].max(levels[1]) } else { levels[bar] };
        let left = rect.left() + bar as f32 * (width + gap);
        let column = Rect::from_min_size(Pos2::new(left, rect.top()), Vec2::new(width, rect.height()));
        painter.rect_filled(column, CornerRadius::same(3), theme::control_background());
        let top = column.bottom() - column.height() * place(db(level));
        let segments = [
            (-48.0, -6.0, Color32::from_rgb(0x22, 0xC5, 0x5E)),
            (-6.0, 0.0, Color32::from_rgb(0xEA, 0xB3, 0x08)),
            (0.0, 6.0, Color32::from_rgb(0xEF, 0x44, 0x44)),
        ];
        for (low, high, color) in segments {
            let bottom = column.bottom() - column.height() * place(low);
            let ceiling = (column.bottom() - column.height() * place(high)).max(top);
            if ceiling < bottom {
                painter.rect_filled(
                    Rect::from_x_y_ranges(column.x_range(), ceiling..=bottom),
                    CornerRadius::same(2),
                    color,
                );
            }
        }
    }
}

/// Draws `peaks` from `from` to `to` seconds across `rect`, what is before `played` dimmed.
fn draw_wave(
    painter: &egui::Painter,
    rect: Rect,
    peaks: &Peaks,
    colors: Option<&suhina_session::WaveColors>,
    (from, to): (f64, f64),
    color: Color32,
    played: Option<f64>,
) {
    let rate = f64::from(peaks.sample_rate.max(1));
    let columns = rect.width().max(1.0) as usize;
    let span = (to - from) / columns as f64;
    let half = rect.height() / 2.0;
    let mut shapes = Vec::with_capacity(columns);
    for column in 0..columns {
        let start = from + span * column as f64;
        let Some(bucket) = peaks.column(Lane::Mixed, start * rate, (start + span) * rate) else { continue };
        let top = bucket.max.clamp(-1.0, 1.0).max(0.02);
        let bottom = bucket.min.clamp(-1.0, 1.0).min(-0.02);
        let x = rect.left() + column as f32 + 0.5;
        // Coloured by what sounds there (lows red, mids green, highs blue), else the deck's.
        let own = colors.and_then(|c| c.column(start, start + span)).map_or(color, band_color);
        let colour = if played.is_some_and(|p| start + span <= p) { own.gamma_multiply(0.35) } else { own };
        shapes.push(egui::Shape::line_segment(
            [Pos2::new(x, rect.center().y - top * half), Pos2::new(x, rect.center().y - bottom * half)],
            Stroke::new(1.0, colour),
        ));
    }
    painter.extend(shapes);
}

/// The colour of a waveform column from its low, mid and high levels: red for the lows, green
/// for the mids, blue for the highs, mixed by how loud each is.
fn band_color([low, mid, high]: [f32; 3]) -> Color32 {
    let total = (low + mid + high).max(1e-6);
    let mix = |l: f32, m: f32, h: f32| (low * l + mid * m + high * h) / total;
    let bright = 0.45 + 0.55 * low.max(mid).max(high);
    let channel = |v: f32| (v * bright).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(
        channel(mix(255.0, 70.0, 80.0)),
        channel(mix(60.0, 220.0, 140.0)),
        channel(mix(90.0, 120.0, 255.0)),
    )
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
    let size = Vec2::new(46.0, 46.0);
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
    let center = Pos2::new(rect.center().x, rect.top() + 16.0);
    let radius = 15.0;
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
        Pos2::new(rect.center().x, rect.bottom() - 5.0),
        Align2::CENTER_CENTER,
        title,
        FontId::proportional(10.0),
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

#[path = "dj_deck_beats.rs"]
mod beats;
#[path = "dj_fx.rs"]
mod fx;
#[path = "dj_library.rs"]
pub(in crate::app) mod library;
#[path = "dj_sampler.rs"]
mod sampler;
#[path = "dj_deck_tools.rs"]
mod tools;

#[cfg(test)]
#[path = "dj_decks_tests.rs"]
mod tests;
