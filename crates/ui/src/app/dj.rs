// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Studio/DJ workspace switch and the separate, asynchronous library.

use super::*;
use serde_json::Value;

#[path = "dj_playlists.rs"]
mod playlists;
#[path = "dj_prepare.rs"]
mod prepare;
#[path = "dj_waveform.rs"]
mod waveform;

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
            if self.dj.preparing.take().is_some() {
                let _ = self.projects.execute("dj.waveform.clear", &json!({}));
            }
            return;
        }
        if !self.dj.snapshot.is_null()
            && self.dj.snapshot["busy"].as_bool() != Some(true)
            && self.dj.pending.is_none()
            && !self.dj.preview_owned
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
                self.dj.snapshot = snapshot;
            }
            Err(e) => {
                self.dj.pending = None;
                self.dj.snapshot = json!({"error": e.to_string()});
            }
        }
        if self.dj.snapshot["busy"].as_bool() == Some(true) || self.dj.pending.is_some() || self.dj.preview_owned {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
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
        ui.separator();
        self.show_dj_playlists(ui, busy);
        ui.add_space(theme::PAD);
        ui.weak("Local library, playlists, audition and cue points. Beat grids, decks, USB export and LINK are still to come.");
    }
}
