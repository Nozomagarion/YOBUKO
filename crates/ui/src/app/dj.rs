// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DJ mode: YOBUKO's own two decks and mixer, drawn in its window (`dj_decks.rs`), with the DJ
//! library under them, and rbxport by Chris Le, shipped inside YOBUKO, preparing USB sticks for
//! CDJ / XDJ players in the USB space, also inside this window (`usb_space.rs`). Nothing in DJ
//! mode opens a window of its own. YOBUKO shares one music library between Mixxx's library (when
//! Mixxx is on this computer) and rbxport (`suhina_dj::mixxx`).

use super::*;

#[path = "dj_decks.rs"]
pub(super) mod decks;
#[path = "dj_flx4.rs"]
pub(super) mod flx4;
#[path = "dj_mixxx.rs"]
pub(super) mod mixxx;
#[path = "dj_mixxx_space.rs"]
pub mod mixxx_space;
#[path = "dj_rbxport.rs"]
pub(super) mod rbxport;

#[derive(Default)]
pub(super) struct DjPanel {
    /// DJ mode shows: Mixxx's page, or the USB page when `usb`.
    pub active: bool,
    /// Of DJ mode's two pages, the USB space (rbxport) shows.
    pub usb: bool,
    /// The next export (File › Export… opened from this page) goes to the DJ library.
    pub next_export_to_dj: bool,
    /// The running export goes to the DJ library.
    pub running_export_to_dj: bool,
    /// What happened last, said on the page: Mixxx or rbxport opened, the library shared, or
    /// why not.
    pub(super) note: Option<Result<String, String>>,
    /// Where requests for rbxport go instead of this user's inbox (tests).
    pub(super) inbox: Option<PathBuf>,
    /// The theme revision and mode last written to `look.json` (`look.rs`).
    pub(super) look_published: Option<(u64, super::menu_bar::WorkspaceMode)>,
    /// The decks, the mixer and the library of the DJ space.
    pub(super) decks: decks::Decks,
    /// The USB space: rbxport inside this window (`usb_space.rs`).
    pub(super) space: super::usb_space::UsbSpace,
    /// Mixxx inside this window, in the DJ space (`dj_mixxx_space.rs`).
    pub(super) mixxx: mixxx_space::MixxxSpace,
}

impl App {
    /// DJ mode starts: the decks and the mixer show in this window. The studio's project stops,
    /// so only the decks play.
    pub(super) fn enter_dj(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        self.dj.active = true;
        self.dj.usb = false;
        self.settings_open = false;
        if self.session().is_some() {
            actions.push(Action::new("transport.stop", json!({})));
        }
        actions.push(Action::new("preview.stop", json!({})));
        // Read the library at the next frame.
        self.dj.decks.snapshot = Value::Null;
        ctx.request_repaint();
    }

    /// The USB space starts: rbxport shows in this window with Mixxx's tracks and playlists, to
    /// put them on a stick for club players (`usb_space.rs`). The studio keeps playing: nothing
    /// here makes sound.
    pub(super) fn enter_usb(&mut self, _ctx: &egui::Context, _actions: &mut Vec<Action>) {
        self.dj.active = true;
        self.dj.usb = true;
        self.settings_open = false;
        self.prepare_usb();
    }

    /// Hands tracks of the DJ library to rbxport's library in the USB space, to put them on a
    /// stick for club players: one request in rbxport's inbox, which it imports.
    pub(in crate::app) fn send_to_usb_space(&mut self, tracks: Vec<PathBuf>) {
        if tracks.is_empty() {
            self.dj.decks.error = Some("No track to send: the files are missing.".into());
            return;
        }
        let sent = match self.rbxport_inbox() {
            Some(inbox) => rbxport::send_to(&inbox, &tracks),
            None => Err(std::io::Error::other("no folder for rbxport's inbox")),
        };
        match sent {
            Ok(_) => {
                self.shell.say(crate::kit::piou::Mood::Joy, format!("{} tracks sent to the USB space.", tracks.len()))
            }
            Err(e) => self.dj.decks.error = Some(format!("Could not send the tracks to the USB space: {e}")),
        }
    }

    fn rbxport_inbox(&self) -> Option<PathBuf> {
        self.dj.inbox.clone().or_else(|| rbxport::user_data_dir().map(|d| d.join("YOBUKO").join("rbxport-inbox")))
    }

    /// Shares the library between Mixxx and rbxport, for the Mixxx this computer has.
    /// `None` when there is no Mixxx (or no data folder) to share with.
    fn share_library(&self, mixxx_closed: bool) -> Option<mixxx::Synced> {
        let found = mixxx::find()?;
        let data = rbxport::user_data_dir()?;
        let (profile, _) =
            mixxx::profile(&found, &data, std::env::var_os("HOME").map(PathBuf::from).as_deref(), &|n| {
                std::env::var(n).ok()
            })?;
        let inbox = self.rbxport_inbox()?;
        Some(mixxx::sync(&profile, &data, &inbox, mixxx_closed))
    }

    /// Shares the library now, without opening anything.
    pub(super) fn share_library_now(&mut self) {
        self.dj.note = Some(match self.share_library(!mixxx::is_running()) {
            None => Err("Mixxx is not on this computer, so there is nothing to share with yet.".into()),
            Some(shared) if shared.problems.is_empty() => Ok(shared.summary()),
            Some(shared) => Err(format!("{}\n• {}", shared.summary(), shared.problems.join("\n• "))),
        });
    }

    /// Before rbxport shows: Mixxx's tracks and playlists are shared with it.
    pub(super) fn prepare_usb(&mut self) {
        self.publish_look(true);
        self.dj.note = self.share_library(!mixxx::is_running()).map(|shared| {
            if shared.problems.is_empty() {
                Ok(shared.summary())
            } else {
                Err(format!("{}\n• {}", shared.summary(), shared.problems.join("\n• ")))
            }
        });
    }

    /// A finished export meant for DJ mode: into the shared library (rbxport now, Mixxx now
    /// when it is closed, else the next time it opens), and Mixxx forward.
    pub(super) fn send_to_dj(&mut self, track: &Path, ctx: &egui::Context) {
        let tracks = [track.to_path_buf()];
        let sent = match &self.dj.inbox {
            Some(inbox) => rbxport::send_to(inbox, &tracks),
            None => rbxport::send(&tracks),
        };
        let name = track.file_name().map_or_else(|| track.display().to_string(), |n| n.to_string_lossy().into());
        if let Err(e) = sent {
            self.dj.note = Some(Err(format!("Could not add {name} to the DJ library: {e}")));
            return;
        }
        let mut in_mixxx = false;
        if let Some(found) = mixxx::find()
            && !mixxx::is_running()
            && let Some(data) = rbxport::user_data_dir()
            && let Some((profile, _)) =
                mixxx::profile(&found, &data, std::env::var_os("HOME").map(PathBuf::from).as_deref(), &|n| {
                    std::env::var(n).ok()
                })
        {
            let rendered =
                suhina_dj::mixxx::MirrorTrack { path: track.to_string_lossy().into_owned(), ..Default::default() };
            in_mixxx = suhina_dj::mixxx::add_to_mixxx(&profile.join(suhina_dj::mixxx::DATABASE), &[rendered]).is_ok();
        }
        self.dj.decks.add([(track.to_path_buf(), None)]);
        ctx.request_repaint();
        let place = if in_mixxx { "Mixxx and rbxport" } else { "rbxport (and Mixxx the next time it opens)" };
        self.shell.say(crate::kit::piou::Mood::Joy, format!("{name} is in your DJ library."));
        self.dj.note = Some(Ok(format!("{name} is in your DJ library, and shared with {place}.")));
    }

    /// What happened last in DJ mode, under a page's buttons.
    pub(super) fn dj_note(&self, ui: &mut Ui) {
        ui.add_space(theme::PAD);
        match &self.dj.note {
            Some(Ok(text)) => {
                ui.label(text);
            }
            Some(Err(text)) => {
                ui.colored_label(theme::accent(), text);
            }
            None => {}
        }
    }
}
