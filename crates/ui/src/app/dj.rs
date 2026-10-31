// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Studio/DJ workspace switch and the separate, asynchronous library.

use super::*;
use serde_json::Value;

#[path = "dj_decks.rs"]
mod decks;
#[path = "dj_grid.rs"]
mod grid;
#[path = "dj_playlists.rs"]
mod playlists;
#[path = "dj_prepare.rs"]
mod prepare;
#[path = "dj_rbxport.rs"]
mod rbxport;
#[path = "dj_waveform.rs"]
mod waveform;
use grid::GridDraft;

#[derive(Default)]
pub(super) struct PlaylistDraft {
    pub(super) id: u64,
    pub(super) revision: u64,
    pub(super) original: String,
    pub(super) name: String,
}

#[derive(Default)]
pub(super) struct DjPanel {
    pub active: bool,
    pub decks_owned: bool,
    pub deck_seek: [f64; 2],
    pub preparing: Option<u64>,
    pub seek: f64,
    pub preview_owned: bool,
    pub preview_started: bool,
    pub cue_selected: Option<u64>,
    pub cue_revision: Option<u64>,
    pub cue_name: String,
    pub cue_seconds: f64,
    pub cue_delete: Option<(u64, u64, u64)>,
    pub cue_queued: bool,
    pub grid_draft: Option<GridDraft>,
    pub grid_delete: Option<(u64, u64)>,
    pub grid_queued: bool,
    pub selected_playlist: Option<u64>,
    pub playlist_name: String,
    pub draft: Option<PlaylistDraft>,
    pub delete_confirmation: Option<(u64, u64)>,
    pub library_page: usize,
    pub playlist_page: usize,
    pub(super) snapshot: serde_json::Value,
    pub(super) next_poll: Option<std::time::Instant>,
    pub pending: Option<(PathBuf, Option<PathBuf>)>,
    pub next_export_to_dj: bool,
    pub running_export_to_dj: bool,
    pub export_source: Option<PathBuf>,
}

impl App {
    pub(super) fn poll_dj(&mut self, ctx: &egui::Context) {
        if !self.dj.active && self.dj.pending.is_none() {
            self.stop_dj_preview();
            self.stop_dj_decks();
            if self.dj.preparing.take().is_some() {
                let _ = self.projects.execute("dj.waveform.clear", &json!({}));
            }
            return;
        }
        if !self.dj.snapshot.is_null()
            && self.dj.snapshot["busy"].as_bool() != Some(true)
            && self.dj.pending.is_none()
            && !self.dj.preview_owned
            && !self.dj.decks_owned
        {
            return;
        }
        let now = std::time::Instant::now();
        if self.dj.next_poll.is_some_and(|at| at > now) {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        self.dj.next_poll = Some(now + std::time::Duration::from_millis(250));
        match self.projects.execute("dj.library", &json!({})) {
            Ok(mut snapshot) => {
                if snapshot["busy"].as_bool() != Some(true)
                    && let Some((path, project)) = self.dj.pending.take()
                {
                    match self.projects.execute("dj.import", &json!({"path": path, "sourceProject": project})) {
                        Ok(_) => {
                            snapshot["busy"] = json!(true);
                            self.status = Status::Hint("Analysing audio for the DJ library…".into());
                        }
                        Err(e) => self.status = Status::Error(e.to_string()),
                    }
                }
                if let Some(id) = snapshot["created_playlist"].as_u64() {
                    self.dj.selected_playlist = Some(id);
                    self.dj.playlist_name.clear();
                    self.dj.draft = None;
                    self.dj.playlist_page = 0;
                }
                if snapshot["preview"]["playing"] == true {
                    self.dj.preview_started = true;
                }
                if self.dj.preview_started && snapshot["preview"]["playing"] == false {
                    self.dj.preview_owned = false;
                    self.dj.preview_started = false;
                }
                if self.dj.cue_queued && snapshot["busy"] != true {
                    self.dj.cue_queued = false;
                    if snapshot["error"].is_null() {
                        self.dj.cue_revision = None;
                    }
                }
                if self.dj.grid_queued && snapshot["busy"] != true {
                    self.dj.grid_queued = false;
                    if snapshot["error"].is_null() {
                        self.dj.grid_draft = None;
                    }
                }
                self.dj.snapshot = snapshot;
            }
            Err(e) => {
                self.dj.pending = None;
                self.dj.snapshot = json!({"error": e.to_string()});
            }
        }
        if self.dj.snapshot["busy"].as_bool() == Some(true)
            || self.dj.pending.is_some()
            || self.dj.preview_owned
            || self.dj.decks_owned
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// Writes the DJ library to the chosen USB stick for Rekordbox players, on the DJ worker.
    pub(super) fn start_dj_usb_export(&mut self, destination: &Path) {
        // The panel's own progress line says how far it has got, and then what the stick holds.
        self.dj_command("dj.usb.export", json!({"destination": destination}));
    }

    /// Hands over to the rbxport app, or opens its download page when it is not installed.
    fn open_rbxport(&mut self, ctx: &egui::Context) {
        match rbxport::installed() {
            Some(app) => match rbxport::launch(&app) {
                Ok(()) => self.status = Status::Hint("Opening rbxport…".into()),
                Err(e) => self.status = Status::Error(format!("could not open rbxport: {e}")),
            },
            None => {
                ctx.open_url(egui::OpenUrl::new_tab(rbxport::DOWNLOAD_PAGE));
                self.status = Status::Hint("rbxport is not installed: its download page is open".into());
            }
        }
    }

    fn show_dj_usb(&mut self, ui: &mut Ui, busy: bool) {
        ui.strong("USB stick for Rekordbox players (CDJ / XDJ)");
        ui.weak("Writes every track, cue and playlist of the DJ library to a stick, the way rekordbox does (rbxport's exporter). A stick exported before is only updated.");
        let has_tracks = self.dj.snapshot["tracks"].as_array().is_some_and(|t| !t.is_empty());
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy && has_tracks, egui::Button::new("Export to USB stick (Rekordbox)…"))
                .on_disabled_hover_text("Import or render a track into the DJ library first")
                .clicked()
            {
                self.ask_pick(PickField::DjUsb);
            }
            let installed = rbxport::installed().is_some();
            let label = if installed { "Open rbxport" } else { "Get rbxport" };
            if ui
                .button(label)
                .on_hover_text("rbxport: the full DJ library app (USB sync, PRO DJ LINK), by Chris Le")
                .clicked()
            {
                self.open_rbxport(ui.ctx());
            }
        });
        let usb = &self.dj.snapshot["usb"];
        if let Some(progress) = usb["progress"].as_object() {
            let stage = progress.get("stage").and_then(Value::as_str).unwrap_or("working");
            let done = progress.get("done").and_then(Value::as_u64).unwrap_or(0);
            let total = progress.get("total").and_then(Value::as_u64).unwrap_or(0);
            let title = progress.get("title").and_then(Value::as_str).unwrap_or("");
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("USB export: {stage} {done}/{total} {title}"));
            });
        } else if let Some(error) = usb["error"].as_str() {
            ui.colored_label(theme::accent(), error);
        } else if let Some(summary) = usb["summary"].as_object() {
            let n = |key: &str| summary.get(key).and_then(Value::as_u64).unwrap_or(0);
            let place = summary.get("destination").and_then(Value::as_str).unwrap_or("the stick");
            ui.label(format!(
                "✔ {} tracks and {} playlists on {place} ({} copied, {} already there, {} converted). Ready for the player.",
                n("tracks"),
                n("playlists"),
                n("copied"),
                n("reused"),
                n("converted"),
            ));
            if let Some(failed) = summary.get("failed").and_then(Value::as_array).filter(|f| !f.is_empty()) {
                ui.colored_label(
                    theme::accent(),
                    format!("Left out: {}", failed.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; ")),
                );
            }
        }
    }

    fn dj_command(&mut self, command: &str, params: Value) {
        match self.projects.execute(command, &params) {
            Ok(_) => {
                self.dj.snapshot["busy"] = json!(true);
                self.dj.snapshot["error"] = Value::Null;
                self.dj.next_poll = None;
                self.dj.delete_confirmation = None;
            }
            Err(e) => self.dj.snapshot["error"] = json!(e.to_string()),
        }
    }

    pub(super) fn show_dj(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().id_salt("dj-workspace").show(ui, |ui| self.show_dj_contents(ui));
    }
    fn show_dj_contents(&mut self, ui: &mut Ui) {
        ui.add_space(theme::PAD);
        ui.heading("DJ library");
        ui.label("Your rendered tracks and imported audio, analysed with rbxport.");
        let busy = self.dj.snapshot["busy"].as_bool() == Some(true) || self.dj.pending.is_some();
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy, egui::Button::new("Import audio…")).clicked() {
                self.ask_pick(PickField::DjAudio);
            }
            if ui
                .add_enabled(!busy && self.session().is_some(), egui::Button::new("Render project to DJ library…"))
                .clicked()
            {
                self.open_export();
                self.dj.next_export_to_dj = true;
            }
            if ui.add_enabled(!busy, egui::Button::new("Refresh DJ library")).clicked() {
                self.dj_command("dj.reload", json!({}));
            }
            if busy {
                ui.spinner();
                ui.label("Loading / analysing…");
            }
        });
        if let Some(error) = self.dj.snapshot["error"].as_str() {
            ui.colored_label(theme::accent(), error);
        }
        ui.add_space(theme::PAD);
        self.show_dj_usb(ui, busy);
        ui.add_space(theme::PAD);
        ui.separator();
        self.show_dj_playlists(ui, busy);
        ui.add_space(theme::PAD);
        self.show_dj_decks(ui);
        ui.add_space(theme::PAD);
        ui.weak("Local library, playlists, cue points, constant beat grids, original-speed A/B decks and Rekordbox USB export. Tempo sync and PRO DJ LINK: use rbxport.");
    }
}
