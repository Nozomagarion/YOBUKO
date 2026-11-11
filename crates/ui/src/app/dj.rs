// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DJ mode: YOBUKO's own two decks and mixer, drawn in its window (`dj_decks.rs`), with the DJ
//! library under them. Mixxx, free DJ software shipped inside YOBUKO, stays a button away for
//! whoever wants it, and rbxport by Chris Le, also shipped inside, prepares USB sticks for CDJ /
//! XDJ players in the USB space. YOBUKO shares one music library between Mixxx and rbxport each
//! time either is opened (`suhina_dj::mixxx`).

use super::*;

#[path = "dj_decks.rs"]
pub(super) mod decks;
#[path = "dj_mixxx.rs"]
pub(super) mod mixxx;
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
    /// The Mixxx YOBUKO started or brought forward: when it closes, the studio comes back.
    pub(super) mixxx_watch: Option<mixxx::Watch>,
    /// The decks, the mixer and the library of the DJ space.
    pub(super) decks: decks::Decks,
}

/// What the DJ page says while Mixxx is open, so nobody looks for a way back.
pub(super) const WAY_BACK: &str = if cfg!(target_os = "macos") {
    "Mixxx fills the screen in YOBUKO's look. To come back to the studio, quit Mixxx (Cmd+Q): YOBUKO comes back by itself."
} else {
    "Mixxx fills the screen in YOBUKO's look. To come back to the studio, quit Mixxx (Ctrl+Q): YOBUKO comes back by itself."
};

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

    /// The USB space starts: rbxport opens with Mixxx's tracks and playlists, to put them on a
    /// stick for club players. The studio keeps playing: nothing here makes sound.
    pub(super) fn enter_usb(&mut self, ctx: &egui::Context, _actions: &mut Vec<Action>) {
        self.dj.active = true;
        self.dj.usb = true;
        self.settings_open = false;
        self.prepare_usb(ctx);
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

    /// Starts Mixxx with the FLX4 set up and the library shared, or opens its download page
    /// when this computer has none.
    pub(super) fn open_mixxx(&mut self, ctx: &egui::Context) {
        let Some(found) = mixxx::find() else {
            ctx.open_url(egui::OpenUrl::new_tab(mixxx::DOWNLOAD_PAGE));
            self.dj.note = Some(Err(
                "Mixxx is not on this computer: its download page is open. Install it, then come back to DJ mode."
                    .into(),
            ));
            return;
        };
        if mixxx::is_running() {
            mixxx::bring_forward(&found);
            if self.dj.mixxx_watch.is_none() {
                self.dj.mixxx_watch = Some(mixxx::Watch::start(ctx.clone()));
            }
            let shared = self.share_library(false).map(|s| s.summary()).unwrap_or_default();
            self.dj.note = Some(Ok(format!("{WAY_BACK} {shared}").trim_end().to_owned()));
            return;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let env = |n: &str| std::env::var(n).ok();
        let mut problems = Vec::new();
        let profile = rbxport::user_data_dir().and_then(|data| mixxx::profile(&found, &data, home.as_deref(), &env));
        if let Some((profile, _)) = &profile {
            let mapping = suhina_dj::mixxx::Mapping::for_version(mixxx::version(&found));
            if let Err(e) = suhina_dj::mixxx::configure(profile, mapping) {
                problems.push(format!("The DDJ-FLX4 could not be set up: {e}"));
            }
        }
        let look = self.publish_look(true);
        if let Some((profile, _)) = &profile
            && let Err(e) = dress_mixxx(&found, profile, &look)
        {
            problems.push(format!("Mixxx could not take YOBUKO's look: {e}"));
        }
        let shared = self.share_library(true);
        if let Some(shared) = &shared {
            problems.extend(shared.problems.iter().cloned());
        }
        let told = profile.as_ref().filter(|(_, told)| *told).map(|(p, _)| p.as_path());
        self.dj.note = Some(match mixxx::launch(&found, told) {
            Ok(()) => {
                self.dj.mixxx_watch = Some(mixxx::Watch::start(ctx.clone()));
                let mut text = String::from(WAY_BACK);
                if let Some(shared) = shared {
                    text.push(' ');
                    text.push_str(&shared.summary());
                }
                for problem in problems {
                    text.push_str("\n• ");
                    text.push_str(&problem);
                }
                Ok(text)
            }
            Err(e) => Err(format!("Could not open Mixxx: {e}")),
        });
    }

    /// Once the Mixxx DJ mode opened has closed: back to the studio, YOBUKO's window in front.
    pub(super) fn after_mixxx(&mut self, ctx: &egui::Context) {
        if !self.dj.mixxx_watch.as_ref().is_some_and(mixxx::Watch::closed) {
            return;
        }
        self.dj.mixxx_watch = None;
        self.come_back_from_mixxx(ctx);
    }

    pub(super) fn come_back_from_mixxx(&mut self, ctx: &egui::Context) {
        self.dj.note = Some(Ok("Mixxx is closed: welcome back to the studio. DJ mode opens it again.".into()));
        // Someone preparing a USB stick meanwhile stays on that page.
        if self.dj.active && !self.dj.usb {
            self.dj.active = false;
            self.multiplayer.open = false;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Shares the library now, without opening anything.
    pub(super) fn share_library_now(&mut self) {
        self.dj.note = Some(match self.share_library(!mixxx::is_running()) {
            None => Err("Mixxx is not on this computer, so there is nothing to share with yet.".into()),
            Some(shared) if shared.problems.is_empty() => Ok(shared.summary()),
            Some(shared) => Err(format!("{}\n• {}", shared.summary(), shared.problems.join("\n• "))),
        });
    }

    /// Opens rbxport to prepare a USB stick, with Mixxx's tracks and playlists sent to it first.
    pub(super) fn prepare_usb(&mut self, ctx: &egui::Context) {
        let shared = self.share_library(!mixxx::is_running());
        self.open_rbxport(ctx);
        if let (Some(Ok(text)), Some(shared)) = (&self.dj.note, shared) {
            self.dj.note = Some(Ok(format!("{text} {}", shared.summary())));
        }
    }

    /// Starts rbxport, or opens its download page when this build has none.
    pub(super) fn open_rbxport(&mut self, ctx: &egui::Context) {
        self.publish_look(true);
        self.dj.note = Some(match rbxport::find() {
            Some(app) => match rbxport::launch(&app) {
                Ok(()) => {
                    Ok("rbxport is open in its own window: choose a playlist and export it to your USB stick.".into())
                }
                Err(e) => Err(format!("Could not open rbxport: {e}")),
            },
            None => {
                ctx.open_url(egui::OpenUrl::new_tab(rbxport::DOWNLOAD_PAGE));
                Err("This copy of YOBUKO has no rbxport inside: its download page is open.".into())
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

    pub(super) fn show_dj(&mut self, ui: &mut Ui) {
        self.show_decks(ui);
    }

    /// The USB space: rbxport, shipped inside YOBUKO, puts the DJ library on a stick for CDJ and
    /// XDJ players.
    pub(super) fn show_usb(&mut self, ui: &mut Ui) {
        use super::menu_bar::WorkspaceMode;
        self.space_page(
            ui,
            WorkspaceMode::Usb,
            "USB export · rbxport",
            "Playing on CDJ or XDJ players? rbxport, which comes with YOBUKO, puts your playlists on a USB \
             stick with their beat grids and hot cues. Your tracks and playlists are the same here and in \
             Mixxx: they are shared each time you open one.",
            |app, ui| {
                if shell::main_button(ui, WorkspaceMode::Usb, "Open rbxport").clicked() {
                    app.prepare_usb(ui.ctx());
                }
                ui.add_space(theme::PAD);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(app.session().is_some(), egui::Button::new("Add this project to the library…"))
                        .on_hover_text("Render the project to audio and add it to your DJ library")
                        .on_disabled_hover_text("Open a project first")
                        .clicked()
                    {
                        app.open_export();
                        app.dj.next_export_to_dj = true;
                    }
                    if ui
                        .button("Share library now")
                        .on_hover_text("Bring Mixxx and rbxport up to date with each other")
                        .clicked()
                    {
                        app.share_library_now();
                    }
                });
                app.dj_note(ui);
                ui.add_space(theme::PAD * 3.0);
                ui.weak("rbxport by Chris Le, free software (GPL-2.0-or-later): github.com/chrisle/rbxport");
            },
        );
    }

    /// What happened last in DJ mode, under a page's buttons.
    fn dj_note(&self, ui: &mut Ui) {
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

/// Gives the Mixxx in `profile` YOBUKO's skin in `look`'s colours, made from the LateNight
/// of the Mixxx at `found` (`suhina_dj::mixxx_skin`). A Mixxx without LateNight keeps its own.
fn dress_mixxx(found: &mixxx::Mixxx, profile: &std::path::Path, look: &suhina_dj::look::Look) -> std::io::Result<()> {
    let Some(source) = suhina_dj::mixxx_skin::source(&found.path) else {
        return Ok(());
    };
    suhina_dj::mixxx_skin::install(&source, profile, look)?;
    suhina_dj::mixxx_skin::select(profile, found.shipped)?;
    Ok(())
}
