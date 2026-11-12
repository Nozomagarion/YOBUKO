// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Explicit LAN starter sessions; the existing project tabs remain local.

use super::*;
use std::sync::Arc;

#[derive(Default, PartialEq)]
enum SharedPage {
    #[default]
    Mixer,
    Tracks,
    Devices,
    Notes,
    Clips,
    Launcher,
    Takes,
}

pub(super) struct MultiplayerPanel {
    pub open: bool,
    pub bound: Option<ProjectId>,
    service: suhina_multiplayer::Service,
    pub(super) online: bool,
    pub(super) address: String,
    join: String,
    invitation: String,
    /// The online host's short code (`7-piano-tigre-lune`); empty on the LAN or until ready.
    short_code: String,
    state: serde_json::Value,
    pub(super) controls: Option<suhina_multiplayer::Controls>,
    draft: Option<suhina_multiplayer::Controls>,
    pub(super) error: Option<String>,
    service_error: Option<String>,
    pub(super) submitted: bool,
    applied_revision: u64,
    pub(super) notes: super::multiplayer_notes::NoteEditor,
    page: SharedPage,
    pub(super) devices: super::multiplayer_devices::DeviceEditor,
    pub(super) clips: super::multiplayer_clips::ClipEditor,
    pub(super) launcher: super::multiplayer_launcher::LauncherEditor,
    pub(super) tracks: super::multiplayer_tracks::TrackEditor,
    take_file: String,
    take_track: u32,
    take_start: f64,
    /// Linked playback: the last shared transport played here, and one still waiting for its moment.
    pub(super) linked_applied: u64,
    linked_pending: Option<(suhina_multiplayer::SharedTransport, Option<std::time::Instant>)>,
    /// Plug-ins the shared tab uses that are not installed here: "Serum (vst3) on Lead".
    pub(super) missing_plugins: Vec<String>,
    /// The invitation last joined with and when, for "Rejoin last session".
    pub(super) last_join: Option<(String, std::time::Instant)>,
    /// Studio edits made in the shared tab, waiting to go to the host in order.
    pub(super) outbox: std::collections::VecDeque<(&'static str, serde_json::Value)>,
}
impl Default for MultiplayerPanel {
    fn default() -> Self {
        Self {
            open: false,
            bound: None,
            service: Default::default(),
            online: true,
            address: suhina_multiplayer::suggested_address().to_string(),
            join: String::new(),
            invitation: String::new(),
            short_code: String::new(),
            state: serde_json::Value::Null,
            controls: None,
            draft: None,
            error: None,
            service_error: None,
            submitted: false,
            applied_revision: 0,
            notes: Default::default(),
            page: SharedPage::Mixer,
            devices: Default::default(),
            clips: Default::default(),
            tracks: Default::default(),
            launcher: Default::default(),
            take_file: String::new(),
            take_track: 0,
            take_start: 0.0,
            linked_applied: 0,
            linked_pending: None,
            missing_plugins: Vec::new(),
            last_join: None,
            outbox: Default::default(),
        }
    }
}
impl MultiplayerPanel {
    /// The running session's mode ("starting", "hosting", "connected", "reconnecting",
    /// "disconnected", "error"), or `None` without a session.
    pub(super) fn mode(&self) -> Option<&str> {
        self.service.active().then(|| self.state["mode"].as_str().unwrap_or("starting"))
    }

    /// How many guests are connected to the session this computer hosts.
    pub(super) fn guests(&self) -> u64 {
        if self.service.active() { self.state["guests_connected"].as_u64().unwrap_or(0) } else { 0 }
    }

    /// The invitation "Rejoin last session" uses: the one last joined with, while it can still
    /// let someone in (an invitation admits guests for 30 minutes).
    pub(super) fn rejoinable(&self) -> Option<&str> {
        const LASTS: std::time::Duration = std::time::Duration::from_secs(30 * 60);
        self.last_join.as_ref().filter(|(_, since)| since.elapsed() < LASTS).map(|(invitation, _)| invitation.as_str())
    }

    /// The online code by default; the typed private address when hosting on the local network.
    fn host_address(&self) -> String {
        if self.online { suhina_multiplayer::ONLINE.to_owned() } else { self.address.clone() }
    }
}
impl App {
    /// Whether the Multiplayer page shows. The shared session itself shows in the Studio, like
    /// any project, with the session's strip above it ([`App::shared_studio_shown`]).
    pub(super) fn multiplayer_visible(&self) -> bool {
        !self.dj.active && self.multiplayer.open
    }

    /// Whether the Studio shows the shared session now: its tab is in front, a session runs and
    /// neither the Multiplayer page nor the DJ space covers it.
    pub(super) fn shared_studio_shown(&self) -> bool {
        !self.dj.active
            && !self.multiplayer.open
            && self.multiplayer.service.active()
            && self.multiplayer.bound.is_some_and(|id| Some(id) == self.projects.active())
    }

    /// Shows the shared session in the Studio: its tab comes to the front.
    pub(super) fn open_shared_studio(&mut self) {
        let Some(id) = self.multiplayer.bound else { return };
        if self.projects.active() != Some(id)
            && let Err(e) = self.projects.execute("project.switch", &json!({"project": id.0}))
        {
            self.multiplayer.error = Some(e.message().to_owned());
            return;
        }
        self.dj.active = false;
        self.multiplayer.open = false;
    }

    /// A Studio action on the shared tab: an edit goes to the session's host, which runs it in
    /// order on every computer's copy (`docs/spec/shared-studio.md`); an edit the session does not
    /// share yet is refused with a sentence. Returns whether the action was handled here; reads,
    /// playback and everything that is not a project command run as usual.
    pub(super) fn shared_studio_action(&mut self, action: &Action) -> bool {
        if self.multiplayer.bound.is_none_or(|id| Some(id) != self.projects.active())
            || !self.multiplayer.service.active()
        {
            return false;
        }
        let kind = suhina_session::commands().iter().find(|c| c.id == action.id).map(|c| c.kind);
        let Some(kind) = kind.filter(|k| k.target == suhina_session::Target::Project) else {
            return false;
        };
        match kind.effect {
            suhina_session::Effect::Edit => {}
            suhina_session::Effect::History => {
                self.status = Status::Hint("Undo and redo are not shared yet: make the opposite edit instead".into());
                return true;
            }
            _ => return false,
        }
        let mut params = action.params.clone();
        if suhina_session::SEEDED.contains(&action.id)
            && let Some(object) = params.as_object_mut()
            && !object.contains_key("seed")
        {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |d| d.subsec_nanos() ^ d.as_secs() as u32);
            object.insert("seed".into(), json!(seed));
        }
        if let Err(error) = suhina_session::shared_studio_params(action.id, &params) {
            self.status = Status::Hint(format!("{error}; leave the session to make this change"));
            return true;
        }
        self.queue_shared_edit(action.id, params);
        true
    }

    /// Queues a studio edit for the host. A drag sends one edit per frame: while the host has not
    /// taken the last one yet, the next one of the same drag replaces it.
    fn queue_shared_edit(&mut self, id: &'static str, params: serde_json::Value) {
        const MOST: usize = 256;
        let outbox = &mut self.multiplayer.outbox;
        let gesture = params.get("gesture").filter(|g| g.is_string()).cloned();
        if let (Some(gesture), Some((last_id, last))) = (gesture, outbox.back_mut())
            && *last_id == id
            && last.get("gesture") == Some(&gesture)
        {
            *last = params;
        } else if outbox.len() < MOST {
            outbox.push_back((id, params));
        } else {
            self.status = Status::Error("too many edits wait for the host; wait a moment".into());
            return;
        }
        self.send_shared_edits();
    }

    /// Hands the waiting studio edits to the session, in order, until it is busy.
    pub(super) fn send_shared_edits(&mut self) {
        while let Some((id, params)) = self.multiplayer.outbox.front() {
            let edit = json!({"command": "studio", "id": id, "params": params}).to_string();
            let request = json!({"base_revision": self.multiplayer.applied_revision, "edit": edit});
            match self.multiplayer.service.execute("multiplayer.edit", &request) {
                Ok(_) => {
                    self.multiplayer.outbox.pop_front();
                }
                Err(e) if e.message().contains("busy") => return,
                Err(e) => {
                    self.multiplayer.outbox.pop_front();
                    self.multiplayer.error = Some(e.message().to_owned());
                }
            }
        }
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
                self.multiplayer.service_error = None;
                if matches!(id, "multiplayer.host" | "multiplayer.hostProject" | "multiplayer.join") {
                    self.multiplayer.controls = None;
                    self.multiplayer.draft = None;
                    self.multiplayer.state = serde_json::Value::Null;
                    self.multiplayer.submitted = false;
                    self.multiplayer.notes = Default::default();
                    self.multiplayer.clips = Default::default();
                    self.multiplayer.tracks = Default::default();
                    self.multiplayer.launcher = Default::default();
                    self.multiplayer.devices = Default::default();
                    self.multiplayer.applied_revision = 0;
                    self.multiplayer.linked_applied = 0;
                    self.multiplayer.linked_pending = None;
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
        let address = self.multiplayer.host_address();
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
        let address = self.multiplayer.host_address();
        self.multiplayer_command("multiplayer.hostProject", json!({"address":address}));
    }
    /// The shared tab's plug-ins that this computer cannot load: each runs the plug-ins installed
    /// here, and the settings travel with the copy.
    fn missing_plugins(&mut self, id: ProjectId) -> Vec<String> {
        let Ok(check) = self.projects.execute("plugin.check", &json!({"project": id.0})) else {
            return Vec::new();
        };
        check["devices"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d.get("found").is_none())
            .map(|d| {
                format!(
                    "{} ({}) on {}",
                    d["device"].as_str().unwrap_or("?"),
                    d["format"].as_str().unwrap_or("?"),
                    d["channel"].as_str().unwrap_or("?")
                )
            })
            .collect()
    }
    fn leave_multiplayer(&mut self) {
        let _ = self.multiplayer.service.execute("multiplayer.stop", &json!({}));
        self.multiplayer.outbox.clear();
        self.multiplayer.missing_plugins.clear();
        if let Some(id) = self.multiplayer.bound.take() {
            let _ = self.projects.restrict_shared_project(id, false);
        }
        self.multiplayer.invitation.clear();
        self.multiplayer.short_code.clear();
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
        self.multiplayer.short_code = state["short_code"].as_str().unwrap_or_default().to_owned();
        let error = state["error"].as_str().map(str::to_owned);
        if error != self.multiplayer.service_error {
            if error.is_some() {
                self.multiplayer.draft = self.multiplayer.controls.clone();
                self.multiplayer.submitted = false;
                self.multiplayer.error = error.clone();
            } else if self.multiplayer.error == self.multiplayer.service_error {
                self.multiplayer.error = None;
            }
            self.multiplayer.service_error = error;
        }
        self.multiplayer.state = state.clone();
        let disconnected = !self.multiplayer.service.active();
        if disconnected && self.multiplayer.bound.is_none() {
            self.multiplayer.invitation.clear();
            self.multiplayer.short_code.clear();
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
        // A guest that came back after a long cut gets a fresh copy: it replaces the shared tab,
        // which held nothing of its own (shared tabs take only confirmed edits).
        let fresh = if self.multiplayer.bound.is_some() { self.multiplayer.service.take_shared() } else { None };
        if fresh.is_some()
            && let Some(old) = self.multiplayer.bound.take()
        {
            let _ = self.projects.restrict_shared_project(old, false);
            let _ = self.projects.close(old);
            self.multiplayer.draft = None;
            self.multiplayer.notes = Default::default();
            self.multiplayer.clips = Default::default();
            self.multiplayer.tracks = Default::default();
            self.multiplayer.launcher = Default::default();
            self.multiplayer.devices = Default::default();
        }
        if self.multiplayer.bound.is_none() {
            let Some(snapshot) = fresh.or_else(|| self.multiplayer.service.take_shared()) else {
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
                    self.multiplayer.missing_plugins = self.missing_plugins(id);
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
                if matches!(
                    operation.edit,
                    suhina_multiplayer::WireEdit::DeviceAdd { .. } | suhina_multiplayer::WireEdit::DeviceRemove { .. }
                ) {
                    self.multiplayer.devices.chain_changed();
                }
                if matches!(
                    operation.edit,
                    suhina_multiplayer::WireEdit::SceneInsert { .. }
                        | suhina_multiplayer::WireEdit::SceneDuplicate { .. }
                        | suhina_multiplayer::WireEdit::SceneDelete { .. }
                ) {
                    self.multiplayer.launcher.structure_changed();
                }
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
        self.send_shared_edits();
        self.follow_linked_transport(ctx, &state);
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
    /// Linked playback: plays each new shared transport on this computer's own player when it is due.
    ///
    /// The service gives the local delay to the scheduled start. A start found late (frames come
    /// about every 100 ms) jumps ahead by the time already played, so every peer stays aligned.
    fn follow_linked_transport(&mut self, ctx: &egui::Context, state: &serde_json::Value) {
        let now = std::time::Instant::now();
        let shared = serde_json::from_value::<Option<suhina_multiplayer::SharedTransport>>(state["transport"].clone())
            .ok()
            .flatten()
            .filter(|t| t.validate().is_ok());
        let Some(shared) = shared else {
            self.multiplayer.linked_pending = None;
            return;
        };
        if shared.seq != self.multiplayer.linked_applied {
            // Refreshed on every poll: the clock estimate improves while the start waits.
            let at = state["transport_starts_in_ms"].as_i64().and_then(|ms| {
                let magnitude = std::time::Duration::from_millis(ms.unsigned_abs());
                if ms >= 0 { now.checked_add(magnitude) } else { now.checked_sub(magnitude) }
            });
            self.multiplayer.linked_pending = Some((shared, at));
        }
        let Some((shared, at)) = self.multiplayer.linked_pending else {
            return;
        };
        // Transport commands play the active tab: wait until the shared tab is in front.
        let Some(id) = self.multiplayer.bound.filter(|id| Some(*id) == self.projects.active()) else {
            return;
        };
        if shared.playing {
            let Some(at) = at else {
                return;
            };
            if let Some(wait) = at.checked_duration_since(now).filter(|d| !d.is_zero()) {
                ctx.request_repaint_after(wait);
                return;
            }
            let late = i64::try_from(now.duration_since(at).as_millis()).unwrap_or(i64::MAX);
            let tempo = self.projects.get(id).map_or(120.0, |s| s.project().transport.tempo_bpm);
            let beat = suhina_multiplayer::beat_after(shared.beat, late, tempo);
            self.run(&Action::new("transport.setPosition", json!({"beats": beat})));
            self.run(&Action::new("transport.play", json!({})));
        } else {
            self.run(&Action::new("transport.stop", json!({})));
            self.run(&Action::new("transport.setPosition", json!({"beats": shared.beat})));
        }
        self.multiplayer.linked_applied = shared.seq;
        self.multiplayer.linked_pending = None;
    }
    /// Play, stop and move: this computer's own player, or every participant's while playback is linked.
    fn show_playback_buttons(&mut self, ui: &mut Ui) {
        let linked = self.multiplayer.state["linked"].as_bool().unwrap_or(false);
        let mut link = linked;
        if ui
            .checkbox(&mut link, "Link playback")
            .on_hover_text("Everyone plays, stops and moves together; each computer still renders the project itself.")
            .changed()
        {
            self.multiplayer_command("multiplayer.linkPlayback", json!({"linked": link}));
        }
        let beat = self.multiplayer.state["transport"]["beat"].as_f64().unwrap_or(0.0);
        ui.horizontal(|ui| {
            if linked {
                if ui.button("Play together").clicked() {
                    self.multiplayer_command("multiplayer.transport", json!({"action":"play","beats":beat}));
                }
                if ui.button("Stop together").clicked() {
                    self.multiplayer_command("multiplayer.transport", json!({"action":"stop","beats":beat}));
                }
                if ui.button("Everyone from beat 16").clicked() {
                    self.multiplayer_command("multiplayer.transport", json!({"action":"seek","beats":16}));
                }
                return;
            }
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
    fn show_publish_take(&mut self, ui: &mut Ui) {
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
        egui::ComboBox::new("publish_take_file", "Loaded take")
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
        egui::ComboBox::new("publish_take_track", "Shared audio track")
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
            ui.label("Add an audio track in the Tracks tab, then choose it here.");
        }
        ui.horizontal(|ui| {
            let label = ui.label("Start beat");
            ui.add(egui::DragValue::new(&mut self.multiplayer.take_start).range(0.0..=120_000.0).speed(0.25))
                .labelled_by(label.id);
        });
        let data = files.iter().find(|(key, _)| key == &self.multiplayer.take_file).map(|(_, data)| Arc::clone(data));
        let ready = data.is_some()
            && tracks.iter().any(|(id, _)| *id == self.multiplayer.take_track)
            && !self.multiplayer.submitted
            && self.multiplayer.state["sending"].is_null();
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
    /// Multiplayer before a session runs: Piou, then two cards, one to open a room and one to
    /// join a friend's. What the buttons do in detail waits behind "How sharing works".
    fn show_multiplayer_lobby(&mut self, ui: &mut Ui) {
        use super::menu_bar::WorkspaceMode;
        self.space_page(
            ui,
            WorkspaceMode::Multiplayer,
            "Multiplayer",
            "Make music together over the Internet, wherever your friends are. One of you opens a room and \
             reads its short code out loud; the others type it. No account needed.",
            |app, ui| {
                ui.columns(2, |columns| {
                    shell::card(&mut columns[0], "Open a room", |ui| app.lobby_host(ui));
                    shell::card(&mut columns[1], "Join a friend", |ui| app.lobby_join(ui));
                });
                app.multiplayer_messages(ui);
                ui.add_space(theme::PAD);
                egui::CollapsingHeader::new("How sharing works").id_salt("multiplayer-how").show(ui, |ui| {
                    ui.weak("Share a project with its loaded audio, share a MIDI-only copy, or create the four-track demo. MIDI notes, clips and mixer controls are shared; each computer has its own playback.");
                    ui.weak("Audio sharing copies loaded sounds into portable WAV files, stored under the application data Shared projects folder. Up to 4096 sounds, 2 GiB each and 16 GiB total. Each computer keeps received sounds in a Sound cache (up to 20 GiB), so a sound is sent only once and a cut transfer resumes. A heavy project opens before all its sounds arrive; their clips stay silent until then. Original files stay intact.");
                    ui.weak("MIDI copies support up to 64 tracks, FM Synth and Utility parameters. Audio, plugins and other instruments are refused. Local metadata and hardware mappings are omitted; the original stays intact.");
                    if app.multiplayer.online {
                        ui.weak("Computers connect directly, through home routers, with nothing to configure. When a router blocks that, a free public relay forwards the encrypted traffic; it cannot read the project. Both computers need the same Suhina version.");
                    } else {
                        ui.weak("Use your private network address for another computer. 127.0.0.1 is only for two instances on this computer; :0 selects a free port.");
                    }
                });
            },
        );
    }

    /// The lobby's first card: where the room is reachable, and what it starts with.
    fn lobby_host(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(&mut self.multiplayer.online, true, "Over the Internet");
            ui.radio_value(&mut self.multiplayer.online, false, "On this local network");
        });
        if !self.multiplayer.online {
            ui.horizontal(|ui| {
                let label = ui.label("Host address");
                ui.add(egui::TextEdit::singleline(&mut self.multiplayer.address).desired_width(200.0).char_limit(64))
                    .labelled_by(label.id);
            });
        }
        ui.add_space(theme::PAD);
        if shell::main_button(ui, super::menu_bar::WorkspaceMode::Multiplayer, "Create session")
            .on_hover_text("Open a room with the four-track demo")
            .clicked()
        {
            let address = self.multiplayer.host_address();
            self.multiplayer_command("multiplayer.host", json!({"address":address}));
        }
        ui.add_space(theme::PAD);
        ui.weak("Or open it with the project showing:");
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.session().is_some(), egui::Button::new("Share project + audio"))
                .on_disabled_hover_text("Open a project first")
                .clicked()
            {
                self.host_current_audio();
            }
            if ui
                .add_enabled(self.session().is_some(), egui::Button::new("Share current MIDI project"))
                .on_disabled_hover_text("Open a project first")
                .clicked()
            {
                self.host_current_midi();
            }
        });
    }

    /// The lobby's second card: back into the session just left in one click, or a code to type.
    fn lobby_join(&mut self, ui: &mut Ui) {
        if let Some(invitation) = self.multiplayer.rejoinable().map(str::to_owned) {
            if shell::main_button(ui, super::menu_bar::WorkspaceMode::Multiplayer, "Rejoin last session")
                .on_hover_text("Go back into the session you were in, with the same invitation")
                .clicked()
            {
                self.join_session(invitation);
            }
            ui.weak("An invitation lets you back in for 30 minutes after the host made it.");
            ui.add_space(theme::PAD);
        }
        let label = ui.label("Invitation code from the host");
        ui.add(
            egui::TextEdit::multiline(&mut self.multiplayer.join)
                .hint_text("Short code such as 7-piano-tigre-lune, or the full invitation")
                .desired_rows(2)
                .char_limit(8192)
                .desired_width(f32::INFINITY),
        )
        .labelled_by(label.id);
        ui.add_space(theme::PAD / 2.0);
        let typed = !self.multiplayer.join.trim().is_empty();
        if ui
            .add_enabled(typed, egui::Button::new("Join session"))
            .on_disabled_hover_text("Type the code first")
            .clicked()
        {
            let invitation = self.multiplayer.join.clone();
            self.join_session(invitation);
        }
    }

    /// Joins the session `invitation` (a short code or a full invitation) leads to, and keeps it
    /// for "Rejoin last session".
    fn join_session(&mut self, invitation: String) {
        self.multiplayer_command("multiplayer.join", json!({"invitation":invitation}));
        if self.multiplayer.error.is_none() {
            self.multiplayer.last_join = Some((invitation.trim().to_owned(), std::time::Instant::now()));
        }
    }

    /// Missing plug-ins and the last refusal, under either page.
    fn multiplayer_messages(&mut self, ui: &mut Ui) {
        if !self.multiplayer.missing_plugins.is_empty() {
            ui.add_space(theme::PAD);
            ui.colored_label(theme::recording(), "Plug-ins missing on this computer");
            for plugin in &self.multiplayer.missing_plugins {
                ui.label(plugin);
            }
            ui.weak("These devices stay silent here; their settings are kept. Install the same plug-ins to hear what the host hears.");
        }
        if let Some(error) = &self.multiplayer.error {
            ui.colored_label(theme::recording(), error);
            if ui.button("Dismiss multiplayer message").clicked() {
                self.multiplayer.error = None;
            }
        }
    }

    /// The strip above the Studio while it shows the shared session: who is in, the code to
    /// read out, the edits still on their way, and the way back to the session's page or out.
    pub(super) fn show_shared_strip(&mut self, ui: &mut Ui) {
        let mode = self.multiplayer.state["mode"].as_str().unwrap_or("starting").to_owned();
        let guests = self.multiplayer.state["guests_connected"].as_u64().unwrap_or(0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(theme::PAD);
            ui.colored_label(theme::accent(), "●");
            ui.strong("Shared session");
            ui.label(status_text(&mode));
            if mode == "hosting" {
                ui.label(format!("· {guests} friend{}", if guests == 1 { "" } else { "s" }));
            }
            if !self.multiplayer.short_code.is_empty() {
                ui.label("· Code");
                ui.label(egui::RichText::new(&self.multiplayer.short_code).monospace().strong());
                if ui.button("Copy code").clicked() {
                    ui.ctx().copy_text(self.multiplayer.short_code.clone());
                }
            }
            let waiting = self.multiplayer.outbox.len();
            if waiting > 0 {
                ui.weak(format!("{waiting} edit(s) on their way"));
            }
            if ui
                .button("Session page")
                .on_hover_text("Invitations, linked playback, takes and the session's other controls")
                .clicked()
            {
                self.multiplayer.open = true;
            }
            if ui.button("Leave session").clicked() {
                self.leave_multiplayer();
            }
        });
        if let Some(error) = self.multiplayer.error.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(theme::PAD);
                ui.colored_label(theme::recording(), error);
                if ui.button("Dismiss multiplayer message").clicked() {
                    self.multiplayer.error = None;
                }
            });
        }
        ui.add_space(theme::PAD / 2.0);
    }

    pub(super) fn show_multiplayer(&mut self, ui: &mut Ui) {
        let active = self.multiplayer.service.active();
        if !active {
            self.show_multiplayer_lobby(ui);
            return;
        }
        ui.add_space(theme::PAD);
        ui.heading("Multiplayer");
        if self.multiplayer.bound.is_some() {
            ui.add_space(theme::PAD / 2.0);
            if shell::main_button(ui, super::menu_bar::WorkspaceMode::Multiplayer, "Open the shared session in Studio")
                .on_hover_text(
                    "The shared project in the Studio, with its arrangement, launcher, clips and devices: \
                     what anyone changes there reaches everyone",
                )
                .clicked()
            {
                self.open_shared_studio();
            }
            ui.weak("Everyone in the session edits the same project in the Studio. The controls below stay available.");
            ui.add_space(theme::PAD / 2.0);
        }
        {
            let mode = self.multiplayer.state["mode"].as_str().unwrap_or("starting").to_owned();
            ui.horizontal(|ui| {
                ui.label(format!("Status: {}", status_text(&mode)));
                if ui.button("Leave session").clicked() {
                    self.leave_multiplayer();
                }
            });
            if !self.multiplayer.short_code.is_empty() {
                ui.horizontal(|ui| {
                    ui.label("Code to read out:");
                    ui.label(egui::RichText::new(&self.multiplayer.short_code).monospace().strong().size(20.0));
                });
            }
            if !self.multiplayer.invitation.is_empty() {
                ui.horizontal(|ui| {
                    if self.multiplayer.short_code.is_empty() {
                        if ui.button("Copy invitation code").clicked() {
                            ui.ctx().copy_text(self.multiplayer.invitation.clone());
                        }
                    } else {
                        if ui.button("Copy code").clicked() {
                            ui.ctx().copy_text(self.multiplayer.short_code.clone());
                        }
                        if ui
                            .button("Copy full invitation")
                            .on_hover_text("The long invitation works when a network blocks the short code's address lookup.")
                            .clicked()
                        {
                            ui.ctx().copy_text(self.multiplayer.invitation.clone());
                        }
                    }
                    let max = suhina_multiplayer::MAX_GUESTS as u64;
                    let seats = self.multiplayer.state["guests_connected"].as_u64().unwrap_or(max)
                        + self.multiplayer.state["guests_reconnecting"].as_u64().unwrap_or(0);
                    let can_renew = seats < max && self.multiplayer.state["sending"].is_null();
                    if ui.add_enabled(can_renew, egui::Button::new("New invitation"))
                        .on_hover_text("Replace the code, for example if it reached someone it should not have. The old code stops working; connected guests stay.")
                        .clicked()
                    {
                        self.multiplayer_command("multiplayer.renewInvitation", json!({}));
                    }

                });
            }
            ui.label(format!(
                "Guests: {} / {}",
                self.multiplayer.state["guests_connected"].as_u64().unwrap_or(0),
                suhina_multiplayer::MAX_GUESTS
            ));
            let away = self.multiplayer.state["guests_reconnecting"].as_u64().unwrap_or(0);
            if away > 0 {
                ui.weak(format!(
                    "{away} guest(s) reconnecting. Their place is kept for two minutes after the connection dropped."
                ));
            }
            if mode == "reconnecting" {
                ui.weak("The connection to the host dropped. Suhina keeps trying for a minute; your edits wait and are sent once it is back.");
            }
            if mode == "hosting" {
                // Keep this row when a guest connects: controls must not move during a click.
                ui.weak("One host and up to seven guests. Send the same code to everyone: it lets each of them in for 30 minutes, until the session is full. New invitation replaces it and the old one stops working; connected guests stay.");
            }
            if self.multiplayer.bound == self.projects.active() {
                self.show_playback_buttons(ui);
            }
            if let Some(progress) = self.multiplayer.state["sending"].as_array() {
                let done = progress.first().and_then(serde_json::Value::as_u64).unwrap_or(0);
                let total = progress.get(1).and_then(serde_json::Value::as_u64).unwrap_or(0);
                if total == 0 {
                    ui.label("Preparing take…");
                } else {
                    ui.label(format!("Sending take: {} / {} KiB", done / 1024, total / 1024));
                }
            }
            if let Some(progress) = self.multiplayer.state["receiving"].as_array() {
                let done = progress.first().and_then(serde_json::Value::as_u64).unwrap_or(0);
                let total = progress.get(1).and_then(serde_json::Value::as_u64).unwrap_or(0);
                ui.label(format!("Receiving project/audio: {} / {} MiB", done >> 20, total >> 20));
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
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Tracks, "Tracks");
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Devices, "Devices");
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Notes, "MIDI notes");
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Clips, "Clips");
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Launcher, "Launcher");
                ui.selectable_value(&mut self.multiplayer.page, SharedPage::Takes, "Publish take");
            });
            if self.multiplayer.page == SharedPage::Launcher {
                self.show_shared_launcher(ui);
            } else if self.multiplayer.page == SharedPage::Devices {
                self.show_shared_devices(ui);
            } else if self.multiplayer.page == SharedPage::Tracks {
                self.show_shared_tracks(ui);
            } else if self.multiplayer.page == SharedPage::Takes {
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
                                        super::multiplayer::named(
                                            ui,
                                            egui::Role::TextInput,
                                            "Track name",
                                            egui::TextEdit::singleline(&mut track.name)
                                                .desired_width(180.0)
                                                .char_limit(256),
                                        );
                                        if ui.button("Rename").clicked() {
                                            edit = Some(
                                                json!({"command":"track.rename","track":track.track,"name":track.name}),
                                            );
                                        }
                                        super::multiplayer::named(
                                            ui,
                                            egui::Role::SpinButton,
                                            "Volume (dB)",
                                            egui::DragValue::new(&mut track.db).range(-150.0..=6.0).speed(0.1),
                                        );
                                        if ui.button("Set volume").clicked() {
                                            edit = Some(
                                                json!({"command":"track.setVolume","track":track.track,"db":track.db}),
                                            );
                                        }
                                        super::multiplayer::named(
                                            ui,
                                            egui::Role::SpinButton,
                                            "Pan",
                                            egui::DragValue::new(&mut track.pan).range(-1.0..=1.0).speed(0.01),
                                        );
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
        self.multiplayer_messages(ui);
        ui.add_space(theme::PAD);
        ui.weak("Original projects are preserved. Edits made in the shared session's Studio reach everyone; undo, pasting, recording and adding plug-ins are not shared yet. Publish finished loaded takes from a local Studio tab. Both participants can publish finished takes. Plug-ins and their settings travel with the copy; each computer runs its own installed plug-ins. Recording directly in the shared tab comes later.");
    }
}

/// The session state as the panel shows it.
pub(super) fn status_text(mode: &str) -> &str {
    match mode {
        "starting" => "Starting…",
        "hosting" => "Hosting",
        "connected" => "Connected",
        "reconnecting" => "Reconnecting…",
        "disconnected" => "Disconnected",
        "error" => "Error",
        other => other,
    }
}

#[cfg(test)]
#[path = "multiplayer_tests.rs"]
mod tests;

/// Adds `widget` under `name`, the name screen readers (and the UI tests) know it by.
pub(crate) fn named(ui: &mut egui::Ui, role: egui::Role, name: &str, widget: impl egui::Widget) -> egui::Response {
    let response = ui.add(widget);
    response.widget_info(|| egui::WidgetInfo::labeled(role, response.enabled(), name));
    response
}
