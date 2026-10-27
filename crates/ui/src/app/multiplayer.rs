// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Explicit LAN starter sessions; the existing project tabs remain local.

use super::*;
use std::sync::Arc;

#[derive(Default, PartialEq)]
enum SharedPage {
    #[default]
    Mixer,
    Notes,
    Clips,
    Takes,
}

pub(super) struct MultiplayerPanel {
    pub open: bool,
    pub bound: Option<ProjectId>,
    service: suhina_multiplayer::Service,
    address: String,
    join: String,
    invitation: String,
    state: serde_json::Value,
    pub(super) controls: Option<suhina_multiplayer::Controls>,
    draft: Option<suhina_multiplayer::Controls>,
    error: Option<String>,
    submitted: bool,
    applied_revision: u64,
    pub(super) notes: super::multiplayer_notes::NoteEditor,
    page: SharedPage,
    pub(super) clips: super::multiplayer_clips::ClipEditor,
    take_file: String,
    take_track: u32,
    take_start: f64,
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
            applied_revision: 0,
            notes: Default::default(),
            page: SharedPage::Mixer,
            clips: Default::default(),
            take_file: String::new(),
            take_track: 0,
            take_start: 0.0,
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
    pub(super) fn multiplayer_command(&mut self, id: &str, params: serde_json::Value) {
        if matches!(id, "multiplayer.host" | "multiplayer.hostProject" | "multiplayer.join") {
            let root = match self.settings.data_folder.trim() {
                "" => self.data_default_folder.clone(),
                folder => PathBuf::from(folder),
            };
            let root = (!root.as_os_str().is_empty()).then(|| root.join("Shared projects"));
            if let Err(error) = self.multiplayer.service.set_storage_root(root) {
                self.multiplayer.error = Some(error.to_string());
                return;
            }
        }
        match self.multiplayer.service.execute(id, &params) {
            Ok(_) => {
                self.multiplayer.error = None;
                if matches!(id, "multiplayer.host" | "multiplayer.hostProject" | "multiplayer.join") {
                    self.multiplayer.controls = None;
                    self.multiplayer.draft = None;
                    self.multiplayer.state = serde_json::Value::Null;
                    self.multiplayer.submitted = false;
                    self.multiplayer.notes = Default::default();
                    self.multiplayer.clips = Default::default();
                    self.multiplayer.applied_revision = 0;
                } else if matches!(id, "multiplayer.edit" | "multiplayer.publishTake") {
                    self.multiplayer.submitted = true;
                }
            }
            Err(e) => self.multiplayer.error = Some(e.to_string()),
        }
    }
    fn host_current_midi(&mut self) {
        let Some(project) = self.projects.active().and_then(|id| self.projects.get(id)).map(|s| s.project()) else {
            self.multiplayer.error = Some("open a MIDI project in Studio first".into());
            return;
        };
        if let Err(error) = self.multiplayer.service.select_project(project) {
            self.multiplayer.error = Some(error.to_string());
            return;
        }
        let address = self.multiplayer.address.clone();
        self.multiplayer_command("multiplayer.hostProject", json!({"address":address}));
    }
    fn host_current_audio(&mut self) {
        let Some(session) = self.projects.active().and_then(|id| self.projects.get(id)) else {
            self.multiplayer.error = Some("open a project in Studio first".into());
            return;
        };
        if let Err(error) = self.multiplayer.service.select_audio_project(session.project(), session.audio()) {
            self.multiplayer.error = Some(error.to_string());
            return;
        }
        let address = self.multiplayer.address.clone();
        self.multiplayer_command("multiplayer.hostProject", json!({"address":address}));
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
        if !self.multiplayer.service.active() && self.multiplayer.bound.is_none() {
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
        let disconnected = !self.multiplayer.service.active();
        if disconnected && self.multiplayer.bound.is_none() {
            self.multiplayer.invitation.clear();
            return;
        }
        let Ok(controls) = serde_json::from_value::<suhina_multiplayer::Controls>(state["controls"].clone()) else {
            if disconnected {
                self.leave_multiplayer();
            }
            return;
        };
        if let Err(e) = controls.validate() {
            self.multiplayer.error = Some(e);
            self.leave_multiplayer();
            return;
        }
        if self.multiplayer.bound.is_none() {
            let Some(snapshot) = self.multiplayer.service.take_shared() else {
                self.multiplayer.error = Some("the verified MIDI project is missing; no project was opened".into());
                self.leave_multiplayer();
                return;
            };
            let revision = snapshot.revision;
            let session = match snapshot.session() {
                Ok(session) => session,
                Err(error) => {
                    self.multiplayer.error = Some(error);
                    self.leave_multiplayer();
                    return;
                }
            };
            match self.projects.add(session, Some("Shared session".into())) {
                Ok(id) => {
                    if let Err(e) = self.projects.restrict_shared_project(id, true) {
                        self.multiplayer.error = Some(e.to_string());
                        self.leave_multiplayer();
                        return;
                    }
                    snapshot.keep();
                    self.multiplayer.bound = Some(id);
                    self.multiplayer.applied_revision = revision;
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
        // Verified sounds reach the existing session before any operation can expose their clip.
        let received = self.multiplayer.service.take_audio();
        for audio in received {
            let result = (|| -> Result<(), String> {
                let folder = audio.folder().ok_or("collected audio folder missing")?;
                self.projects
                    .get_mut(id)
                    .ok_or("shared project closed")?
                    .add_collected_audio(&audio.audio, folder)
                    .map_err(|e| e.to_string())
            })();
            if let Err(error) = result {
                self.multiplayer.error = Some(error);
                self.leave_multiplayer();
                return;
            }
            audio.keep();
        }
        let operations = serde_json::from_value::<Vec<suhina_multiplayer::Operation>>(state["operations"].clone());
        let result = (|| -> Result<(), String> {
            let operations = operations.map_err(|_| "invalid shared operation list")?;
            if operations.len() > 128 {
                return Err("shared operation queue overflow".into());
            }
            for operation in operations {
                if operation.revision <= self.multiplayer.applied_revision {
                    continue;
                }
                if self.multiplayer.applied_revision.checked_add(1) != Some(operation.revision) {
                    return Err("shared operation revision gap".into());
                }
                operation.edit.edit()?;
                let (command, params) = operation.edit.command();
                self.projects.apply_shared_control(id, command, &params).map_err(|e| e.to_string())?;
                self.multiplayer.applied_revision = operation.revision;
            }
            let session = self.projects.get(id).ok_or("shared project is closed")?;
            if controls.revision != self.multiplayer.applied_revision
                || !controls.changes(session.project())?.is_empty()
            {
                return Err("shared operations do not match the host snapshot".into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.multiplayer.error = Some(format!("{error}; the last local copy was preserved"));
            self.leave_multiplayer();
            return;
        }
        if self.multiplayer.controls.as_ref() == Some(&controls) {
            if disconnected {
                self.leave_multiplayer();
            }
            return;
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
        if disconnected {
            self.leave_multiplayer();
        }
    }
    fn show_publish_take(&mut self, ui: &mut Ui) {
        if self.multiplayer.state["mode"] != "hosting" {
            ui.label("The host publishes finished takes. You receive their sound before the new clip appears.");
            return;
        }
        ui.label("Record in an ordinary local Studio tab, finish the take, then choose its loaded sound here.");
        ui.weak("Publishes the whole sound, unwarped, with default gain. Clip trims, loops and effects are not baked in. Original takes stay intact.");
        let source = self
            .projects
            .active()
            .filter(|id| Some(*id) != self.multiplayer.bound)
            .and_then(|id| self.projects.get(id))
            .filter(|s| !s.is_recording());
        let files: Vec<_> = source
            .into_iter()
            .flat_map(|s| {
                let refs: std::collections::BTreeSet<_> = s
                    .project()
                    .contents
                    .iter()
                    .flat_map(|c| &c.lanes)
                    .filter_map(|l| match &l.data {
                        suhina_engine::model::LaneData::Audio(a) if !a.file.is_empty() => Some(a.file.clone()),
                        _ => None,
                    })
                    .collect();
                s.audio()
                    .files
                    .iter()
                    .filter(|(key, _)| refs.contains(*key))
                    .map(|(key, data)| (key.clone(), Arc::clone(data)))
                    .collect::<Vec<_>>()
            })
            .collect();
        if files.is_empty() {
            ui.label("Select an ordinary local project tab with a finished, loaded audio take. Leave this panel open while switching tabs.");
        }
        egui::ComboBox::from_id_salt("publish_take_file")
            .selected_text(if self.multiplayer.take_file.is_empty() {
                "Choose loaded take"
            } else {
                Path::new(&self.multiplayer.take_file).file_name().and_then(|s| s.to_str()).unwrap_or("Loaded sound")
            })
            .show_ui(ui, |ui| {
                for (key, _) in &files {
                    let label = Path::new(key).file_name().and_then(|s| s.to_str()).unwrap_or(key);
                    ui.selectable_value(&mut self.multiplayer.take_file, key.clone(), label);
                }
            });
        let tracks: Vec<_> = self
            .multiplayer
            .bound
            .and_then(|id| self.projects.get(id))
            .into_iter()
            .flat_map(|s| {
                s.project()
                    .tracks
                    .iter()
                    .filter(|t| t.content.audio && !t.content.notes)
                    .map(|t| (t.id.0, t.name.clone()))
            })
            .collect();
        egui::ComboBox::from_id_salt("publish_take_track")
            .selected_text(
                tracks
                    .iter()
                    .find(|(id, _)| *id == self.multiplayer.take_track)
                    .map_or("Choose shared audio track", |(_, name)| name.as_str()),
            )
            .show_ui(ui, |ui| {
                for (id, name) in &tracks {
                    ui.selectable_value(&mut self.multiplayer.take_track, *id, name);
                }
            });
        if tracks.is_empty() {
            ui.label("The shared project needs an existing audio-only track. Adding shared tracks comes later.");
        }
        ui.horizontal(|ui| {
            ui.label("Start beat");
            ui.add(egui::DragValue::new(&mut self.multiplayer.take_start).range(0.0..=120_000.0).speed(0.25));
        });
        let data = files.iter().find(|(key, _)| key == &self.multiplayer.take_file).map(|(_, data)| Arc::clone(data));
        let ready = data.is_some()
            && tracks.iter().any(|(id, _)| *id == self.multiplayer.take_track)
            && !self.multiplayer.submitted;
        if ui.add_enabled(ready, egui::Button::new("Publish loaded take")).clicked()
            && let Some(data) = data
        {
            match self.multiplayer.service.select_take(data) {
                Ok(()) => self.multiplayer_command(
                    "multiplayer.publishTake",
                    json!({"base_revision":self.multiplayer.applied_revision,
                    "track":self.multiplayer.take_track,"start":self.multiplayer.take_start}),
                ),
                Err(error) => self.multiplayer.error = Some(error.to_string()),
            }
        }
        if self.multiplayer.submitted {
            ui.label("Waiting for the host to confirm publication…");
        }
    }
    pub(super) fn show_multiplayer(&mut self, ui: &mut Ui) {
        let active = self.multiplayer.service.active();
        ui.add_space(theme::PAD);
        ui.heading("Multiplayer · LAN session");
        ui.label("Create a session, copy its invitation, and join from the same local network.");
        ui.weak("Share a project with its loaded audio, share a MIDI-only copy, or create the four-track demo. MIDI notes, clips and mixer controls are shared; each computer has its own playback.");
        if !active {
            ui.horizontal(|ui| {
                ui.label("Host address");
                ui.add(egui::TextEdit::singleline(&mut self.multiplayer.address).desired_width(200.0).char_limit(64));
                if ui.button("Create LAN session").clicked() {
                    let address = self.multiplayer.address.clone();
                    self.multiplayer_command("multiplayer.host", json!({"address":address}));
                }
            });
            if ui.add_enabled(self.session().is_some(), egui::Button::new("Share current MIDI project")).clicked() {
                self.host_current_midi();
            }
            if ui.add_enabled(self.session().is_some(), egui::Button::new("Share project + audio")).clicked() {
                self.host_current_audio();
            }
            ui.weak("Audio sharing copies loaded sounds into portable WAV files, stored under the application data Shared projects folder. Up to 64 sounds, 64 MiB each and 256 MiB total. Original files stay intact.");
            ui.weak("MIDI copies support up to 64 tracks, FM Synth and Utility parameters. Audio, plugins and other instruments are refused. Local metadata and hardware mappings are omitted; the original stays intact.");
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
            if let Some(progress) = self.multiplayer.state["receiving"].as_array() {
                let done = progress.first().and_then(serde_json::Value::as_u64).unwrap_or(0);
                let total = progress.get(1).and_then(serde_json::Value::as_u64).unwrap_or(0);
                ui.label(format!("Receiving project/audio: {} / {} KiB", done / 1024, total / 1024));
            }
            if let Some(session) = self.multiplayer.bound.and_then(|id| self.projects.get(id)) {
                let project = session.project();
                let notes: usize = project
                    .contents
                    .iter()
                    .flat_map(|c| &c.lanes)
                    .map(|l| match &l.data {
                        suhina_engine::model::LaneData::Notes(n) => n.len(),
                        _ => 0,
                    })
                    .sum();
                ui.label(format!(
                    "Shared copy: {} tracks · {} MIDI notes · {} audio files. Edit notes and clip placement below.",
                    project.tracks.len(),
                    notes,
                    session.audio().files.len()
                ));
            }
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Mixer, "Mixer");
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Notes, "MIDI notes");
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Clips, "Clips");
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Takes, "Publish take");
            });
            if self.multiplayer.page == SharedPage::Takes {
                self.show_publish_take(ui);
            } else if self.multiplayer.page == SharedPage::Clips {
                self.show_shared_clips(ui);
            } else if self.multiplayer.page == SharedPage::Notes {
                self.show_shared_notes(ui);
            } else {
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
                    egui::ScrollArea::vertical()
                        .id_salt("lan-controls-scroll")
                        .max_height((ui.available_height() - 80.0).max(120.0))
                        .show(ui, |ui| {
                            egui::Grid::new("lan-track-controls").striped(true).spacing(Vec2::new(12.0, 10.0)).show(
                                ui,
                                |ui| {
                                    for title in ["Track name", "", "Volume (dB)", "", "Pan", ""] {
                                        ui.strong(title);
                                    }
                                    ui.end_row();
                                    for track in &mut draft.tracks {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut track.name)
                                                .desired_width(180.0)
                                                .char_limit(256),
                                        );
                                        if ui.button("Rename").clicked() {
                                            edit = Some(
                                                json!({"command":"track.rename","track":track.track,"name":track.name}),
                                            );
                                        }
                                        ui.add(egui::DragValue::new(&mut track.db).range(-150.0..=6.0).speed(0.1));
                                        if ui.button("Set volume").clicked() {
                                            edit = Some(
                                                json!({"command":"track.setVolume","track":track.track,"db":track.db}),
                                            );
                                        }
                                        ui.add(egui::DragValue::new(&mut track.pan).range(-1.0..=1.0).speed(0.01));
                                        if ui.button("Set pan").clicked() {
                                            edit = Some(
                                                json!({"command":"track.setPan","track":track.track,"pan":track.pan}),
                                            );
                                        }
                                        ui.end_row();
                                    }
                                },
                            );
                        });
                    ui.weak(format!("Confirmed revision {confirmed}. Apply one control at a time; the host confirms or refuses concurrent edits."));
                }
                if let Some(edit) = edit {
                    self.multiplayer_command("multiplayer.edit", json!({"base_revision":base,"edit":edit.to_string()}));
                }
            }
        }
        if let Some(error) = &self.multiplayer.error {
            ui.colored_label(theme::recording(), error);
        }
        ui.add_space(theme::PAD);
        ui.weak("Original projects are preserved. Leave to edit this copy freely. Publish finished loaded takes from a local Studio tab. Guest uploads, recording directly in the shared tab and external plugins come later.");
    }
}

#[cfg(test)]
#[path = "multiplayer_tests.rs"]
mod tests;
