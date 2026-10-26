// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Explicit LAN starter sessions; the existing project tabs remain local.

use super::*;

pub(super) struct MultiplayerPanel {
    pub open: bool,
    pub bound: Option<ProjectId>,
    service: suhina_multiplayer::Service,
    address: String,
    join: String,
    invitation: String,
    state: serde_json::Value,
    controls: Option<suhina_multiplayer::Controls>,
    draft: Option<suhina_multiplayer::Controls>,
    error: Option<String>,
    submitted: bool,
}
impl Default for MultiplayerPanel {
    fn default() -> Self {
        Self {
            open: false,
            bound: None,
            service: Default::default(),
            address: suhina_multiplayer::suggested_address().to_string(),
            join: String::new(),
            invitation: String::new(),
            state: serde_json::Value::Null,
            controls: None,
            draft: None,
            error: None,
            submitted: false,
        }
    }
}
impl App {
    pub(super) fn multiplayer_visible(&self) -> bool {
        !self.dj.active
            && (self.multiplayer.open
                || self
                    .multiplayer
                    .bound
                    .is_some_and(|id| Some(id) == self.projects.active() && self.multiplayer.service.active()))
    }
    fn multiplayer_command(&mut self, id: &str, params: serde_json::Value) {
        match self.multiplayer.service.execute(id, &params) {
            Ok(_) => {
                self.multiplayer.error = None;
                if matches!(id, "multiplayer.host" | "multiplayer.join") {
                    self.multiplayer.controls = None;
                    self.multiplayer.draft = None;
                    self.multiplayer.state = serde_json::Value::Null;
                    self.multiplayer.submitted = false;
                } else if id == "multiplayer.edit" {
                    self.multiplayer.submitted = true;
                }
            }
            Err(e) => self.multiplayer.error = Some(e.to_string()),
        }
    }
    fn leave_multiplayer(&mut self) {
        let _ = self.multiplayer.service.execute("multiplayer.stop", &json!({}));
        if let Some(id) = self.multiplayer.bound.take() {
            let _ = self.projects.restrict_shared_project(id, false);
        }
        self.multiplayer.invitation.clear();
        self.multiplayer.join.clear();
    }
    pub(super) fn poll_multiplayer(&mut self, ctx: &egui::Context) {
        if !self.multiplayer.service.active() {
            if let Some(id) = self.multiplayer.bound.take() {
                let _ = self.projects.restrict_shared_project(id, false);
            }
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        let state = match self.multiplayer.service.execute("multiplayer.state", &json!({})) {
            Ok(state) => state,
            Err(e) => {
                self.multiplayer.error = Some(e.to_string());
                return;
            }
        };
        self.multiplayer.invitation = state["invitation"].as_str().unwrap_or_default().to_owned();
        let error = state["error"].as_str().map(str::to_owned);
        if error.is_some() && error != self.multiplayer.error {
            self.multiplayer.draft = self.multiplayer.controls.clone();
            self.multiplayer.submitted = false;
        }
        self.multiplayer.error = error;
        self.multiplayer.state = state.clone();
        if !self.multiplayer.service.active() {
            if let Some(id) = self.multiplayer.bound.take() {
                let _ = self.projects.restrict_shared_project(id, false);
            }
            self.multiplayer.invitation.clear();
            return;
        }
        let Ok(controls) = serde_json::from_value::<suhina_multiplayer::Controls>(state["controls"].clone()) else {
            return;
        };
        if let Err(e) = controls.validate() {
            self.multiplayer.error = Some(e);
            self.leave_multiplayer();
            return;
        }
        if self.multiplayer.bound.is_none() {
            match self
                .projects
                .add(Session::new(suhina_multiplayer::starter_project()), Some("LAN starter session".into()))
            {
                Ok(id) => {
                    if let Err(e) = self.projects.restrict_shared_project(id, true) {
                        self.multiplayer.error = Some(e.to_string());
                        self.leave_multiplayer();
                        return;
                    }
                    self.multiplayer.bound = Some(id);
                    self.multiplayer.controls = None;
                }
                Err(e) => {
                    self.multiplayer.error = Some(e.to_string());
                    self.leave_multiplayer();
                    return;
                }
            }
        }
        let Some(id) = self.multiplayer.bound else {
            return;
        };
        let Some(session) = self.projects.get(id) else {
            self.leave_multiplayer();
            return;
        };
        if let Some(previous) = &self.multiplayer.controls {
            if controls.revision < previous.revision || controls.revision == previous.revision && controls != *previous
            {
                self.multiplayer.error = Some("shared revision mismatch; the session was left".into());
                self.leave_multiplayer();
                return;
            }
            if controls == *previous {
                return;
            }
        }
        let changes = match controls.changes(session.project()) {
            Ok(changes) => changes,
            Err(e) => {
                self.multiplayer.error = Some(e);
                self.leave_multiplayer();
                return;
            }
        };
        for (command, params) in changes {
            if let Err(e) = self.projects.apply_shared_control(id, command, &params) {
                self.multiplayer.error = Some(format!("shared control failed: {e}; rejoin with a new invitation"));
                self.leave_multiplayer();
                return;
            }
        }
        self.revision = self.revision.saturating_add(1);
        if self.multiplayer.submitted
            || !ctx.egui_wants_keyboard_input() && !ctx.input(|i| i.pointer.any_down())
            || self.multiplayer.draft.is_none()
        {
            self.multiplayer.draft = Some(controls.clone());
        }
        self.multiplayer.submitted = false;
        self.multiplayer.controls = Some(controls);
    }
    pub(super) fn show_multiplayer(&mut self, ui: &mut Ui) {
        let active = self.multiplayer.service.active();
        ui.add_space(theme::PAD);
        ui.heading("Multiplayer · LAN starter session");
        ui.label("Create a session, copy its invitation, and join from the same local network.");
        ui.weak("This first session shares four generated MIDI tracks: names, volumes and pan. Each computer has its own playback.");
        if !active {
            ui.horizontal(|ui| {
                ui.label("Host address");
                ui.add(egui::TextEdit::singleline(&mut self.multiplayer.address).desired_width(200.0).char_limit(64));
                if ui.button("Create LAN session").clicked() {
                    let address = self.multiplayer.address.clone();
                    self.multiplayer_command("multiplayer.host", json!({"address":address}));
                }
            });
            ui.weak("Use your private network address for another computer. 127.0.0.1 is only for two instances on this computer; :0 selects a free port.");
            ui.add_space(theme::PAD);
            ui.label("Invitation from the host");
            ui.add(
                egui::TextEdit::multiline(&mut self.multiplayer.join)
                    .desired_rows(3)
                    .char_limit(8192)
                    .desired_width(f32::INFINITY),
            );
            if ui.button("Join LAN session").clicked() {
                let invitation = self.multiplayer.join.clone();
                self.multiplayer_command("multiplayer.join", json!({"invitation":invitation}));
            }
        } else {
            let mode = self.multiplayer.state["mode"].as_str().unwrap_or("starting").to_owned();
            ui.horizontal(|ui| {
                ui.label(format!("Status: {mode}"));
                if ui.button("Leave session").clicked() {
                    self.leave_multiplayer();
                }
            });
            if !self.multiplayer.invitation.is_empty() {
                ui.horizontal(|ui| {
                    if ui.button("Copy invitation").clicked() {
                        ui.ctx().copy_text(self.multiplayer.invitation.clone());
                    }
                    ui.label(if self.multiplayer.state["guest_connected"] == true {
                        "Guest connected"
                    } else {
                        "Waiting for guest · invitation expires after 5 minutes and is used once"
                    });
                });
            }
            if self.multiplayer.bound == self.projects.active() {
                ui.horizontal(|ui| {
                    if ui.button("Play locally").clicked() {
                        self.run(&Action::new("transport.play", json!({})));
                    }
                    if ui.button("Stop locally").clicked() {
                        self.run(&Action::new("transport.stop", json!({})));
                    }
                    if ui.button("Listen from beat 16").clicked() {
                        self.run(&Action::new("transport.setPosition", json!({"beats":16})));
                    }
                });
            }
            let base = self.multiplayer.draft.as_ref().map_or(0, |c| c.revision);
            let confirmed = self.multiplayer.controls.as_ref().map_or(0, |c| c.revision);
            if base != confirmed {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "New controls arrived (revision {confirmed}). Your pending edit still uses revision {base}."
                    ));
                    if ui.button("Refresh controls").clicked() {
                        self.multiplayer.draft = self.multiplayer.controls.clone();
                    }
                });
            }
            let mut edit = None;
            if let Some(draft) = self.multiplayer.draft.as_mut() {
                ui.separator();
                egui::Grid::new("lan-track-controls").striped(true).spacing(Vec2::new(12.0, 10.0)).show(ui, |ui| {
                    for title in ["Track name", "", "Volume (dB)", "", "Pan", ""] {
                        ui.strong(title);
                    }
                    ui.end_row();
                    for track in &mut draft.tracks {
                        ui.push_id(track.track, |ui| {
                            ui.add(egui::TextEdit::singleline(&mut track.name).desired_width(180.0).char_limit(256));
                            if ui.button("Rename").clicked() {
                                edit = Some(json!({"command":"track.rename","track":track.track,"name":track.name}));
                            }
                            ui.add(egui::DragValue::new(&mut track.db).range(-150.0..=6.0).speed(0.1));
                            if ui.button("Set volume").clicked() {
                                edit = Some(json!({"command":"track.setVolume","track":track.track,"db":track.db}));
                            }
                            ui.add(egui::DragValue::new(&mut track.pan).range(-1.0..=1.0).speed(0.01));
                            if ui.button("Set pan").clicked() {
                                edit = Some(json!({"command":"track.setPan","track":track.track,"pan":track.pan}));
                            }
                            ui.end_row();
                        });
                    }
                });
                ui.weak(format!("Confirmed revision {confirmed}. Apply one control at a time; the host confirms or refuses concurrent edits."));
            }
            if let Some(edit) = edit {
                self.multiplayer_command("multiplayer.edit", json!({"base_revision":base,"edit":edit.to_string()}));
            }
        }
        if let Some(error) = &self.multiplayer.error {
            ui.colored_label(theme::recording(), error);
        }
        ui.add_space(theme::PAD);
        ui.weak("Other projects are preserved. Leave this session to edit its notes and clips freely. Sharing your own projects, audio files and recordings comes later.");
    }
}

#[cfg(test)]
#[path = "multiplayer_tests.rs"]
mod tests;
