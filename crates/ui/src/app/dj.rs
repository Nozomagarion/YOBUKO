// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DJ mode: Mixxx, free DJ software shipped inside YOBUKO, to mix live with a controller
//! (the Pioneer DDJ-FLX4 is set up by itself), and rbxport by Chris Le, also shipped inside,
//! to prepare USB sticks for CDJ / XDJ players. Mixxx fills the screen in YOBUKO's look, and
//! quitting it brings the studio back; rbxport opens in its own window. YOBUKO shares one music
//! library between them each time either is opened (`suhina_dj::mixxx`).

use super::*;

#[path = "dj_mixxx.rs"]
pub(super) mod mixxx;
#[path = "dj_rbxport.rs"]
pub(super) mod rbxport;

#[derive(Default)]
pub(super) struct DjPanel {
    pub active: bool,
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
}

/// What the DJ page says while Mixxx is open, so nobody looks for a way back.
pub(super) const WAY_BACK: &str = if cfg!(target_os = "macos") {
    "Mixxx fills the screen in YOBUKO's look. To come back to the studio, quit Mixxx (Cmd+Q): YOBUKO comes back by itself."
} else {
    "Mixxx fills the screen in YOBUKO's look. To come back to the studio, quit Mixxx (Ctrl+Q): YOBUKO comes back by itself."
};

impl App {
    /// DJ mode starts: Mixxx opens, or comes forward when it is already open.
    pub(super) fn enter_dj(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        self.dj.active = true;
        self.settings_open = false;
        if self.session().is_some() {
            actions.push(Action::new("transport.stop", json!({})));
        }
        actions.push(Action::new("preview.stop", json!({})));
        self.open_mixxx(ctx);
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
        if self.dj.active {
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
        self.open_mixxx(ctx);
        let place = if in_mixxx { "Mixxx and rbxport" } else { "rbxport (and Mixxx the next time it opens)" };
        let opened = match &self.dj.note {
            Some(Ok(text)) => format!(" {text}"),
            _ => String::new(),
        };
        self.dj.note = Some(Ok(format!("{name} is in your DJ library: {place}.{opened}")));
    }

    pub(super) fn show_dj(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().id_salt("dj-workspace").show(ui, |ui| {
            ui.add_space(theme::PAD * 4.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(600.0);
                ui.heading("DJ mode · Mixxx");
                ui.add_space(theme::PAD);
                ui.label(
                    "Mix live in Mixxx, which comes with YOBUKO and fills the screen in YOBUKO's look. Plug in your \
                     controller: a Pioneer DDJ-FLX4 is set up by itself, and Mixxx knows hundreds of others \
                     (choose yours once in Mixxx › Preferences › Controllers).",
                );
                ui.add_space(theme::PAD);
                ui.label(
                    "Playing on CDJ or XDJ players? rbxport, also inside YOBUKO, prepares the USB stick. Your \
                     tracks, playlists and hot cues are shared between the two each time you open one.",
                );
                ui.add_space(theme::PAD * 2.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Open Mixxx").clicked() {
                        self.open_mixxx(ui.ctx());
                    }
                    if ui
                        .button("Prepare a USB stick (rbxport)")
                        .on_hover_text(
                            "Open rbxport with your Mixxx tracks and playlists, to export them for CDJ / XDJ",
                        )
                        .clicked()
                    {
                        self.prepare_usb(ui.ctx());
                    }
                    if ui
                        .add_enabled(self.session().is_some(), egui::Button::new("Send this project to DJ…"))
                        .on_hover_text("Render the project to audio and add it to your DJ library")
                        .on_disabled_hover_text("Open a project first")
                        .clicked()
                    {
                        self.open_export();
                        self.dj.next_export_to_dj = true;
                    }
                    if ui
                        .button("Share library now")
                        .on_hover_text("Bring Mixxx and rbxport up to date with each other")
                        .clicked()
                    {
                        self.share_library_now();
                    }
                });
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
                ui.add_space(theme::PAD * 3.0);
                ui.weak("Mixxx by the Mixxx team, free software (GPL-2.0-or-later): mixxx.org");
                ui.weak("rbxport by Chris Le, free software (GPL-2.0-or-later): github.com/chrisle/rbxport");
                ui.weak("DDJ-FLX4 mapping by Robert904 and ElHanko (MIT): github.com/ElHanko/mixxx-ddj-flx4-mapping");
            });
        });
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
