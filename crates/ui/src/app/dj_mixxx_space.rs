// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! YOBUKO: the DJ space shows Mixxx itself (free DJ software, mixxx.org, shipped inside
//! YOBUKO) in this window, in YOBUKO's look, with every one of its decks, buttons, menus and
//! settings. The desktop app starts Mixxx and puts its window over the rectangle this page
//! leaves for it (`apps/suhina/src/mixxx_panel.rs`, through the plug-in host's `--adopt`);
//! this page draws a slim strip above it and what to say while Mixxx starts or when it cannot.
//!
//! Where another program's window cannot be shown inside this one (macOS, a Wayland session),
//! or when the DJ asks for them, YOBUKO's own decks show instead (`dj_decks.rs`).
//!
//! The page and the desktop app talk through [`App::set_mixxx_host`] (how far Mixxx is),
//! [`App::take_mixxx_view`] (where its window goes this frame), [`App::take_mixxx_asks`] (what
//! was picked here) and [`App::mixxx_launch`] (the Mixxx to start, set up for YOBUKO).

use std::ffi::OsString;
use std::path::PathBuf;

use egui::{Rect, Ui, Vec2};

use super::*;

/// How far Mixxx is in the DJ space.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MixxxStatus {
    /// Not started yet: the DJ space has not been opened.
    #[default]
    Off,
    Starting,
    /// Its window is in this one.
    Ready,
    /// It cannot show inside this window here (why): YOBUKO's own decks show.
    Unsupported(String),
    /// No Mixxx in this copy of YOBUKO nor on this computer.
    Missing,
    /// It could not start, or it closed: why.
    Stopped(String),
}

/// What the desktop app says about Mixxx, every frame it changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MixxxHost {
    pub status: MixxxStatus,
}

/// Where Mixxx's window goes this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct MixxxView {
    /// In points; the desktop app scales it to the window's pixels.
    pub rect: Rect,
    /// Something of YOBUKO's (a menu, a dialog, Settings) lies over the rectangle: Mixxx steps
    /// aside, since its window would hide it.
    pub covered: bool,
}

/// What was picked on the page, for the desktop app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixxxAsk {
    /// Start Mixxx again after it stopped.
    Restart,
}

/// The Mixxx to start, set up for YOBUKO: its program, its arguments and its profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixxxLaunch {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// Said on the page once it runs: the library shared, or what could not be set up.
    pub note: String,
}

/// The DJ space's share of the window for Mixxx, kept in the DJ panel.
#[derive(Default)]
pub(in crate::app) struct MixxxSpace {
    pub(in crate::app) host: MixxxHost,
    pub(in crate::app) view: Option<MixxxView>,
    pub(in crate::app) asks: Vec<MixxxAsk>,
    /// The DJ asked for YOBUKO's own decks instead of Mixxx.
    pub(in crate::app) own_decks: bool,
    /// Whether a Mixxx was found, looked at once, the first time the DJ space shows (never in
    /// tests, which see YOBUKO's decks).
    pub(in crate::app) found: Option<bool>,
}

impl MixxxSpace {
    /// Mixxx is (or will be) in the DJ space, rather than YOBUKO's own decks.
    pub(in crate::app) fn wanted(&self) -> bool {
        !self.own_decks
            && self.found != Some(false)
            && !matches!(self.host.status, MixxxStatus::Unsupported(_) | MixxxStatus::Missing)
    }
}

/// The radius of the page's soft shapes, as the DJ space's.
const RADIUS: u8 = 10;

/// The program inside a macOS app bundle, which is started as a program rather than opened.
fn program(found: &mixxx::Mixxx) -> PathBuf {
    if found.path.extension().is_some_and(|ext| ext == "app") {
        found.path.join("Contents").join("MacOS").join("Mixxx")
    } else {
        found.path.clone()
    }
}

impl App {
    /// The Mixxx to start in the DJ space, set up for YOBUKO first: the DDJ-FLX4 mapping, the
    /// YOBUKO skin in the current theme's colours, the library shared with rbxport.
    pub fn mixxx_launch(&mut self) -> Option<MixxxLaunch> {
        let found = mixxx::find()?;
        self.lend_controllers_to_mixxx(true);
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let env = |n: &str| std::env::var(n).ok();
        let mut problems = Vec::new();
        let profile =
            super::dj::rbxport::user_data_dir().and_then(|data| mixxx::profile(&found, &data, home.as_deref(), &env));
        if let Some((profile, _)) = &profile {
            // Its music folder: the DJ library's first one, else the person's Music folder.
            let folder = self
                .dj
                .decks
                .folders()
                .into_iter()
                .next()
                .or_else(|| home.as_ref().map(|h| h.join("Music")).filter(|m| m.is_dir()))
                .or_else(|| home.clone());
            if let Some(folder) = folder
                && let Err(e) = suhina_dj::mixxx::seed_music_folder(profile, &folder)
            {
                problems.push(format!("Mixxx could not be given its music folder: {e}"));
            }
            let mapping = suhina_dj::mixxx::Mapping::for_version(mixxx::version(&found));
            if let Err(e) = suhina_dj::mixxx::configure(profile, mapping) {
                problems.push(format!("The DDJ-FLX4 could not be set up: {e}"));
            }
            let look = self.publish_look(true);
            let dressed = suhina_dj::mixxx_skin::source(&found.path).map_or(Ok(()), |source| {
                suhina_dj::mixxx_skin::install(&source, profile, &look)?;
                suhina_dj::mixxx_skin::select(profile, found.shipped).map(drop)
            });
            if let Err(e) = dressed {
                problems.push(format!("Mixxx could not take YOBUKO's look: {e}"));
            }
        }
        let mut note = String::new();
        if let Some(shared) = self.share_library(true) {
            note = shared.summary();
            problems.extend(shared.problems);
        }
        for problem in problems {
            note.push_str("\n• ");
            note.push_str(&problem);
        }
        let told = profile.as_ref().filter(|(_, told)| *told).map(|(p, _)| p.as_path());
        Some(MixxxLaunch { program: program(&found), args: mixxx::arguments(told), note: note.trim().to_owned() })
    }

    /// How far Mixxx is, from the desktop app.
    pub fn set_mixxx_host(&mut self, host: MixxxHost) {
        if self.dj.mixxx.host != host {
            match &host.status {
                MixxxStatus::Stopped(why) => self.dj.note = Some(Err(why.clone())),
                MixxxStatus::Ready if matches!(self.dj.note, Some(Err(_))) => self.dj.note = None,
                _ => {}
            }
            self.lend_controllers_to_mixxx(matches!(host.status, MixxxStatus::Starting | MixxxStatus::Ready));
            self.dj.mixxx.host = host;
        }
    }

    /// While Mixxx runs, the DJ controllers (a DDJ-FLX4) are its: YOBUKO closes their ports,
    /// which Windows would otherwise refuse to Mixxx, and opens them again once it stops.
    fn lend_controllers_to_mixxx(&mut self, lend: bool) {
        let ports = if lend {
            self.projects.midi_ports().iter().filter(|p| super::flx4::is_flx4(p)).cloned().collect()
        } else {
            std::collections::BTreeSet::new()
        };
        let _notices = self.projects.lend_midi_ports(ports);
    }

    /// How far Mixxx is, as last set.
    pub fn mixxx_host(&self) -> &MixxxHost {
        &self.dj.mixxx.host
    }

    /// Whether the DJ space shows Mixxx: it starts the first time it does.
    pub fn mixxx_space_shown(&self) -> bool {
        self.workspace_mode() == super::menu_bar::WorkspaceMode::Dj && self.dj.mixxx.wanted()
    }

    /// Where Mixxx's window goes this frame; `None` when the DJ space does not show it, and it
    /// hides. Taken once a frame, after [`App::ui`].
    pub fn take_mixxx_view(&mut self, ctx: &egui::Context) -> Option<MixxxView> {
        let mut view = self.dj.mixxx.view.take()?;
        view.covered = super::usb_space::covered(ctx, view.rect);
        Some(view)
    }

    /// What was picked on the Mixxx page since the last call.
    pub fn take_mixxx_asks(&mut self) -> Vec<MixxxAsk> {
        std::mem::take(&mut self.dj.mixxx.asks)
    }

    /// The DJ space: Mixxx, or YOBUKO's own decks where Mixxx cannot show or the DJ asked.
    pub(in crate::app) fn show_dj(&mut self, ui: &mut Ui) {
        if self.dj.mixxx.found.is_none() {
            self.dj.mixxx.found = Some(mixxx::find().is_some());
        }
        if self.dj.mixxx.wanted() {
            self.show_mixxx(ui);
            return;
        }
        if self.dj.mixxx.own_decks {
            ui.horizontal(|ui| {
                ui.add_space(theme::PAD);
                if ui
                    .add(egui::Button::new("Back to Mixxx").corner_radius(RADIUS))
                    .on_hover_text("Mix in Mixxx inside this window, with all of its decks, effects and settings")
                    .clicked()
                {
                    self.dj.mixxx.own_decks = false;
                }
            });
        }
        self.show_decks(ui);
    }

    fn show_mixxx(&mut self, ui: &mut Ui) {
        use super::menu_bar::WorkspaceMode;
        let status = self.dj.mixxx.host.status.clone();
        if status != MixxxStatus::Ready {
            self.show_mixxx_waiting(ui, &status);
            return;
        }
        let outer = ui.available_rect_before_wrap().shrink(theme::PAD);
        let strip_height = 30.0;
        let strip = Rect::from_min_size(outer.min, Vec2::new(outer.width(), strip_height));
        let body = Rect::from_min_max(egui::pos2(outer.left(), strip.bottom() + theme::PAD / 2.0), outer.max);
        ui.painter().rect_filled(strip, RADIUS, theme::panel_background());
        ui.painter().rect_stroke(strip, RADIUS, egui::Stroke::new(1.0, theme::border()), egui::StrokeKind::Inside);
        let mut strip_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(strip.shrink2(Vec2::new(8.0, 3.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        strip_ui.label(egui::RichText::new("Mixxx").strong().color(WorkspaceMode::Dj.color()));
        strip_ui.weak("inside YOBUKO");
        strip_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new("YOBUKO decks").corner_radius(RADIUS))
                .on_hover_text("Use YOBUKO's own decks instead of Mixxx (Mixxx keeps running)")
                .clicked()
            {
                self.dj.mixxx.own_decks = true;
            }
            if let Some(note) = &self.dj.note {
                let (text, color) = match note {
                    Ok(text) => (text.as_str(), theme::text_secondary()),
                    Err(text) => (text.as_str(), theme::accent()),
                };
                let first = text.lines().next().unwrap_or_default();
                ui.add(egui::Label::new(egui::RichText::new(first).small().color(color)).truncate())
                    .on_hover_text(text);
            }
        });
        // The card Mixxx's window fills: rounded, in the space's colour along its edge.
        ui.painter().rect_filled(body, RADIUS, theme::panel_background());
        ui.painter().rect_stroke(
            body,
            RADIUS,
            egui::Stroke::new(1.5, WorkspaceMode::Dj.color().gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
        ui.allocate_rect(outer, egui::Sense::hover());
        self.dj.mixxx.view = Some(MixxxView { rect: body.shrink(3.0), covered: false });
    }

    fn show_mixxx_waiting(&mut self, ui: &mut Ui, status: &MixxxStatus) {
        use super::menu_bar::WorkspaceMode;
        let about = match status {
            MixxxStatus::Stopped(why) => format!("Mixxx stopped: {why}"),
            _ => "Mixxx is starting inside this window, in YOBUKO's look: every deck, effect, sampler and \
                  setting of Mixxx, with your DDJ-FLX4 ready."
                .to_owned(),
        };
        let stopped = matches!(status, MixxxStatus::Stopped(_));
        self.space_page(ui, WorkspaceMode::Dj, "DJ · Mixxx", &about, |app, ui| {
            if stopped && shell::main_button(ui, WorkspaceMode::Dj, "Start Mixxx again").clicked() {
                app.dj.mixxx.asks.push(MixxxAsk::Restart);
            }
            if ui.add(egui::Button::new("Use YOBUKO decks").corner_radius(RADIUS)).clicked() {
                app.dj.mixxx.own_decks = true;
            }
            app.dj_note(ui);
            ui.add_space(theme::PAD * 3.0);
            ui.weak("Mixxx, free DJ software (GPL-2.0-or-later): mixxx.org");
        });
    }
}

#[cfg(test)]
#[path = "dj_mixxx_space_tests.rs"]
mod tests;
