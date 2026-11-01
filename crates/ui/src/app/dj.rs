// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DJ mode: rbxport, the DJ library app by Chris Le, shipped inside YOBUKO. Switching to DJ
//! starts it (or brings it forward); this page says so and sends rendered projects to it.

use super::*;

#[path = "dj_rbxport.rs"]
pub(super) mod rbxport;

#[derive(Default)]
pub(super) struct DjPanel {
    pub active: bool,
    /// The next export (File › Export… opened from this page) goes to rbxport's library.
    pub next_export_to_dj: bool,
    /// The running export goes to rbxport's library.
    pub running_export_to_dj: bool,
    /// What happened last, said on the page: rbxport opened, a track sent, or why not.
    pub(super) note: Option<Result<String, String>>,
    /// Where requests for rbxport go instead of this user's inbox (tests).
    pub(super) inbox: Option<PathBuf>,
}

impl App {
    /// DJ mode starts: rbxport opens, or comes forward when it is already open.
    pub(super) fn enter_dj(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        self.dj.active = true;
        self.settings_open = false;
        if self.session().is_some() {
            actions.push(Action::new("transport.stop", json!({})));
        }
        actions.push(Action::new("preview.stop", json!({})));
        self.open_rbxport(ctx);
    }

    /// Starts rbxport, or opens its download page when this build has none.
    pub(super) fn open_rbxport(&mut self, ctx: &egui::Context) {
        self.dj.note = Some(match rbxport::find() {
            Some(app) => match rbxport::launch(&app) {
                Ok(()) => Ok("rbxport is open in its own window.".into()),
                Err(e) => Err(format!("Could not open rbxport: {e}")),
            },
            None => {
                ctx.open_url(egui::OpenUrl::new_tab(rbxport::DOWNLOAD_PAGE));
                Err("This copy of YOBUKO has no rbxport inside: its download page is open.".into())
            }
        });
    }

    /// A finished export meant for DJ mode: into rbxport's library, and rbxport forward.
    pub(super) fn send_to_rbxport(&mut self, track: &Path, ctx: &egui::Context) {
        let tracks = [track.to_path_buf()];
        let sent = match &self.dj.inbox {
            Some(inbox) => rbxport::send_to(inbox, &tracks),
            None => rbxport::send(&tracks),
        };
        match sent {
            Ok(_) => {
                self.open_rbxport(ctx);
                let name =
                    track.file_name().map_or_else(|| track.display().to_string(), |n| n.to_string_lossy().into());
                self.dj.note = Some(Ok(format!("{name} is on its way to rbxport's collection.")));
            }
            Err(e) => self.dj.note = Some(Err(format!("Could not send the track to rbxport: {e}"))),
        }
    }

    pub(super) fn show_dj(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().id_salt("dj-workspace").show(ui, |ui| {
            ui.add_space(theme::PAD * 4.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(560.0);
                ui.heading("DJ mode · rbxport");
                ui.add_space(theme::PAD);
                ui.label(
                    "Your DJ library, track analysis, decks, USB sticks for CDJ / XDJ players and PRO DJ LINK \
                     all live in rbxport, which comes with YOBUKO and opens in its own window.",
                );
                ui.add_space(theme::PAD * 2.0);
                ui.horizontal(|ui| {
                    if ui.button("Open rbxport").clicked() {
                        self.open_rbxport(ui.ctx());
                    }
                    if ui
                        .add_enabled(self.session().is_some(), egui::Button::new("Send this project to rbxport…"))
                        .on_hover_text("Render the project to audio and add it to rbxport's collection")
                        .on_disabled_hover_text("Open a project first")
                        .clicked()
                    {
                        self.open_export();
                        self.dj.next_export_to_dj = true;
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
                ui.weak("rbxport by Chris Le, free software (GPL-2.0-or-later): github.com/chrisle/rbxport");
            });
        });
    }
}
