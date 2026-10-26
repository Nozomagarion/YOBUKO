// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Studio/DJ workspace switch and the separate, asynchronous library.

use super::*;

#[derive(Default)]
pub(super) struct DjPanel {
    pub active: bool,
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
            return;
        }
        if !self.dj.snapshot.is_null() && self.dj.snapshot["busy"].as_bool() != Some(true) && self.dj.pending.is_none()
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
                self.dj.snapshot = snapshot;
            }
            Err(e) => {
                self.dj.pending = None;
                self.dj.snapshot = json!({"error": e.to_string()});
            }
        }
        if self.dj.snapshot["busy"].as_bool() == Some(true) || self.dj.pending.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    pub(super) fn show_dj(&mut self, ui: &mut Ui) {
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
        let tracks = self.dj.snapshot["tracks"].as_array().cloned().unwrap_or_default();
        if tracks.is_empty() && !busy {
            ui.label("Import an audio file, or open a project and render it into this library.");
        }
        egui::ScrollArea::vertical().id_salt("dj-library").show(ui, |ui| {
            egui::Grid::new("dj-tracks").striped(true).spacing(Vec2::new(20.0, 10.0)).show(ui, |ui| {
                for text in ["Track", "BPM", "Key", "Length", "Source"] {
                    ui.strong(text);
                }
                ui.end_row();
                for track in tracks {
                    ui.label(track["title"].as_str().unwrap_or("Untitled"))
                        .on_hover_text(track["path"].as_str().unwrap_or_default());
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
                    ui.end_row();
                }
            });
        });
        ui.add_space(theme::PAD);
        ui.weak("First integration: local library and analysis. Playlists, cues, decks, USB export and LINK are still to come.");
    }
}
