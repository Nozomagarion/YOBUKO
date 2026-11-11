// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! YOBUKO's shell: what stays the same whichever space shows. One tab per space (Studio,
//! Multiplayer, DJ, USB) at the right end of the bar, the session's state beside them once a
//! shared session runs, and Piou, the mascot (`kit::piou`), whose mood follows the app: it nods
//! while music plays, calls while a room waits for friends, hops when one arrives and worries
//! when the connection drops. What happens away from the page that would say it (a friend
//! joining while the studio shows) is told by Piou in a short note at the bottom of the window.

use super::menu_bar::WorkspaceMode;
use super::*;
use crate::kit::piou::{self, Coat, Mood};

/// How long one of Piou's notes stays, and how long it takes to fade in and out.
const NOTE_SECONDS: f64 = 4.0;
const NOTE_FADE: f64 = 0.2;
/// How long Piou hops after good news or a click on it.
const JOY_SECONDS: f64 = 2.2;
/// Without a key or a click for this long, Piou falls asleep.
const SLEEP_AFTER: f64 = 300.0;

#[derive(Default)]
pub(super) struct Shell {
    /// What Piou says now.
    pub(super) note: Option<Note>,
    /// The shared session as it was last frame (`None` before the first look), to tell what
    /// changed.
    seen: Option<Seen>,
    /// Until when Piou hops (`ui.input` time).
    joy_until: f64,
    /// Good news that arrived where no clock was at hand: Piou hops from the next frame.
    joy_asked: bool,
    /// When a key or the pointer was last used.
    last_input: f64,
}

/// One of Piou's notes: what happened, and the mood it comes with.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Note {
    pub text: String,
    pub mood: Mood,
    /// When it appeared; set the first time it is drawn.
    since: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Seen {
    mode: Option<String>,
    guests: u64,
}

impl Shell {
    /// Has Piou say `text` in `mood` for a few seconds.
    pub(super) fn say(&mut self, mood: Mood, text: impl Into<String>) {
        self.note = Some(Note { text: text.into(), mood, since: None });
        if mood == Mood::Joy {
            self.joy_asked = true;
        }
    }
}

impl WorkspaceMode {
    /// Piou in this space: its headphones in a lighter shade of the space's colour.
    pub(super) fn coat(self) -> Coat {
        match self {
            Self::Studio => Coat::cream(egui::Color32::from_rgb(0x5B, 0x8C, 0xFF)),
            Self::Multiplayer => Coat::yellow(egui::Color32::from_rgb(0x3F, 0xD6, 0xA0)),
            Self::Dj => Coat::lavender(egui::Color32::from_rgb(0xFF, 0x6F, 0xB5)),
            Self::Usb => Coat::cream(egui::Color32::from_rgb(0xFF, 0xB2, 0x3F)),
        }
    }
}

impl App {
    /// The spaces as tabs at the bar's right end, with YOBUKO's seal, Piou and the shared
    /// session's state to their left. The tab of the space showing is filled in its colour;
    /// clicking another switches to it.
    pub(super) fn workspace_modes(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        let current = self.workspace_mode();
        self.watch_session();
        let row = ui.max_rect();
        let font = theme::ui_font(theme::TEXT_SMALL);
        let height = (row.height() - 4.0).clamp(14.0, 22.0);
        let labels: Vec<_> = WorkspaceMode::ALL
            .iter()
            .map(|mode| ui.painter().layout_no_wrap(mode.label().to_owned(), font.clone(), egui::Color32::WHITE))
            .collect();
        // A tab: its dot, its name, and room around them.
        const DOT: f32 = 6.0;
        const SIDE: f32 = 8.0;
        const GAP: f32 = 5.0;
        let widths: Vec<f32> = labels.iter().map(|g| SIDE + DOT + GAP + g.size().x + SIDE).collect();
        let total: f32 = widths.iter().sum();
        let track = Rect::from_min_size(
            Pos2::new(row.right() - total - 4.0, row.center().y - height / 2.0),
            Vec2::new(total, height),
        );
        self.paint_identity(ui, current, track);
        self.publish_look(false);
        ui.painter().rect_filled(track, height / 2.0, theme::window_background());

        // The fill behind the tab of the space showing slides to it.
        let index = WorkspaceMode::ALL.iter().position(|mode| *mode == current).unwrap_or(0);
        let offset: f32 = widths[..index].iter().sum();
        let slide = motion::value(ui.ctx(), Id::unique("space-tab-slide"), offset, 0.18);
        let width = motion::value(ui.ctx(), Id::unique("space-tab-width"), widths[index], 0.18);
        let lit = Rect::from_min_size(track.min + Vec2::new(slide, 0.0), Vec2::new(width, height));
        ui.painter().rect_filled(lit, height / 2.0, current.color());

        let mut picked = None;
        let mut left = track.left();
        for ((mode, galley), width) in WorkspaceMode::ALL.into_iter().zip(labels).zip(widths) {
            let rect = Rect::from_min_size(Pos2::new(left, track.top()), Vec2::new(width, height));
            left += width;
            let selected = mode == current;
            let response = ui.interact(rect, Id::unique(("space-tab", mode.label())), Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::Button, true, selected, format!("{} space", mode.label()))
            });
            let hover = widgets::hover(ui, &response);
            let text = if selected {
                egui::Color32::WHITE
            } else {
                motion::mix(theme::text_secondary(), theme::text_primary(), hover)
            };
            let dot = if selected { egui::Color32::WHITE } else { mode.color() };
            ui.painter().circle_filled(Pos2::new(rect.left() + SIDE + DOT / 2.0, rect.center().y), DOT / 2.0, dot);
            let at = Pos2::new(rect.left() + SIDE + DOT + GAP, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley_with_override_text_color(at, galley, text);
            if response.clicked() && !selected {
                picked = Some(mode);
            }
        }

        // Piou, left of the seal: its mood at a glance. A click makes it hop.
        let seal = (height - 2.0).max(12.0);
        let side = row.height().min(24.0);
        let bird = Rect::from_center_size(
            Pos2::new(track.left() - 6.0 - seal - 6.0 - side / 2.0, row.center().y),
            Vec2::splat(side),
        );
        let mood = self.piou_mood(ui.ctx());
        let response = widgets::button_area(ui, bird, Id::unique("shell-piou"), "Piou");
        if response.clicked() {
            self.shell.joy_until = ui.input(|i| i.time) + JOY_SECONDS;
        }
        piou::show(ui.ctx(), ui.painter(), bird, mood, current.coat(), self.beat_seconds());

        self.session_chip(ui, bird.left() - 8.0, height, &mut picked);
        self.piou_note(ui.ctx(), current);
        if let Some(mode) = picked {
            self.run_menu_item(mode.menu_item(), ui.ctx(), actions);
        }
    }

    /// The shared session's state, ending at `right`: shown in every space while a session
    /// runs, so nobody has to open Multiplayer to know whether friends are still there. A click
    /// opens Multiplayer.
    fn session_chip(&self, ui: &Ui, right: f32, height: f32, picked: &mut Option<WorkspaceMode>) {
        let Some(mode) = self.multiplayer.mode() else {
            return;
        };
        let guests = self.multiplayer.guests();
        let text = match mode {
            "hosting" if guests == 1 => "Room open · 1 friend".to_owned(),
            "hosting" if guests > 1 => format!("Room open · {guests} friends"),
            "hosting" => "Room open".to_owned(),
            "connected" => "In a session".to_owned(),
            other => multiplayer::status_text(other).to_owned(),
        };
        let colour = match mode {
            "hosting" | "connected" => theme::meter_normal(),
            "reconnecting" | "starting" => theme::meter_hot(),
            _ => theme::recording(),
        };
        let row = ui.max_rect();
        let galley =
            ui.painter().layout_no_wrap(text.clone(), theme::ui_font(theme::TEXT_SMALL), theme::text_primary());
        let size = Vec2::new(galley.size().x + 24.0, height);
        let rect = Rect::from_min_size(Pos2::new(right - size.x, row.center().y - height / 2.0), size);
        // Too narrow a window keeps the menus readable and drops the chip.
        if rect.left() < row.left() + 320.0 {
            return;
        }
        let response = widgets::button_area(ui, rect, Id::unique("shell-session-chip"), format!("Session: {text}"));
        let hover = widgets::hover(ui, &response);
        let fill = motion::mix(theme::control_background(), theme::control_hover(), hover);
        ui.painter().rect_filled(rect, height / 2.0, fill);
        // The dot breathes while the session is not settled.
        let alive = if matches!(mode, "reconnecting" | "starting") { 0.45 + 0.55 * motion::pulse(ui.ctx()) } else { 1.0 };
        ui.painter().circle_filled(Pos2::new(rect.left() + 9.0, rect.center().y), 3.0, colour.gamma_multiply(alive));
        ui.painter().galley(Pos2::new(rect.left() + 16.0, rect.center().y - galley.size().y / 2.0), galley, theme::text_primary());
        if response.clicked() {
            *picked = Some(WorkspaceMode::Multiplayer);
        }
    }

    /// Piou's mood now, from what the app is doing.
    pub(super) fn piou_mood(&mut self, ctx: &egui::Context) -> Mood {
        let (time, used) = ctx.input(|i| (i.time, !i.events.is_empty() || i.pointer.any_down()));
        if used || self.shell.last_input == 0.0 {
            self.shell.last_input = time;
        }
        if std::mem::take(&mut self.shell.joy_asked) {
            self.shell.joy_until = time + JOY_SECONDS;
        }
        let session = self.multiplayer.mode();
        if session == Some("reconnecting") {
            Mood::Lost
        } else if time < self.shell.joy_until {
            Mood::Joy
        } else if session == Some("hosting") && self.multiplayer.guests() == 0 {
            Mood::Call
        } else if self.session().and_then(|s| s.player()).is_some_and(|p| p.monitor().running()) {
            Mood::Play
        } else if time - self.shell.last_input > SLEEP_AFTER {
            Mood::Sleep
        } else {
            Mood::Idle
        }
    }

    /// The length of a beat of the project showing, for Piou's nod.
    pub(super) fn beat_seconds(&self) -> f64 {
        self.session().map_or(0.5, |s| 60.0 / s.project().transport.tempo_bpm.max(20.0))
    }

    /// Tells, through Piou, what changed in the shared session since the last frame.
    fn watch_session(&mut self) {
        let now = Seen { mode: self.multiplayer.mode().map(str::to_owned), guests: self.multiplayer.guests() };
        let Some(before) = self.shell.seen.replace(now.clone()) else {
            return;
        };
        if before == now {
            return;
        }
        if let Some((mood, text)) = news(&before, &now) {
            self.shell.say(mood, text);
        }
    }

    /// Piou's note, at the bottom right of the window above the status bar. It takes no click:
    /// what is under it stays usable.
    fn piou_note(&mut self, ctx: &egui::Context, space: WorkspaceMode) {
        let time = ctx.input(|i| i.time);
        let Some(note) = &mut self.shell.note else {
            return;
        };
        let since = *note.since.get_or_insert(time);
        let age = time - since;
        if age > NOTE_SECONDS {
            self.shell.note = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((NOTE_SECONDS - age).clamp(0.02, 0.25)));
        let shown = if motion::enabled(ctx) {
            (age / NOTE_FADE).min((NOTE_SECONDS - age) / NOTE_FADE).clamp(0.0, 1.0) as f32
        } else {
            1.0
        };
        let (text, mood) = (note.text.clone(), note.mood);
        let beat = self.beat_seconds();
        egui::Area::new(Id::unique("piou-note"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-12.0, -(theme::STATUS_BAR + 12.0) + 8.0 * (1.0 - shown)))
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_opacity(shown);
                egui::Frame::NONE
                    .fill(theme::panel_background())
                    .stroke(egui::Stroke::new(1.0, theme::border_strong()))
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (bird, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
                            piou::show(ui.ctx(), ui.painter(), bird, mood, space.coat(), beat);
                            ui.set_max_width(300.0);
                            ui.label(egui::RichText::new(text).color(theme::text_primary()));
                        });
                    });
            });
    }

    /// The top of a space's page: Piou, the page's name and what the space is for, centred,
    /// then `body` (its buttons and notes) under them.
    pub(super) fn space_page(
        &mut self,
        ui: &mut Ui,
        space: WorkspaceMode,
        heading: &str,
        about: &str,
        body: impl FnOnce(&mut Self, &mut Ui),
    ) {
        let mood = self.piou_mood(ui.ctx());
        let beat = self.beat_seconds();
        egui::ScrollArea::vertical().id_salt(("space-page", space.label())).show(ui, |ui| {
            ui.add_space(theme::PAD * 3.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(620.0);
                let (bird, _) = ui.allocate_exact_size(Vec2::splat(96.0), Sense::hover());
                piou::show(ui.ctx(), ui.painter(), bird, mood, space.coat(), beat);
                ui.add_space(theme::PAD);
                ui.heading(heading);
                ui.add_space(theme::PAD / 2.0);
                ui.label(about);
                ui.add_space(theme::PAD * 2.0);
                body(self, ui);
            });
        });
    }
}

/// What to tell when the shared session went from `before` to `now`, and in which mood.
fn news(before: &Seen, now: &Seen) -> Option<(Mood, &'static str)> {
    let (was, is) = (before.mode.as_deref(), now.mode.as_deref());
    if is == Some("hosting") && was == Some("hosting") && now.guests > before.guests {
        Some((Mood::Joy, "A friend joined your session."))
    } else if is == Some("connected") && was == Some("reconnecting") {
        Some((Mood::Joy, "Back online. Nothing was lost."))
    } else if is == Some("connected") && was != Some("connected") {
        Some((Mood::Joy, "You are in the session."))
    } else if is == Some("reconnecting") && was != Some("reconnecting") {
        Some((Mood::Lost, "Connection lost. Reconnecting by itself: your edits are kept."))
    } else if is == Some("disconnected") && was.is_some_and(|mode| mode != "disconnected") {
        Some((Mood::Lost, "The session ended. Rejoin it from Multiplayer."))
    } else {
        None
    }
}

/// A space's main action: a wide button filled in the space's colour.
pub(super) fn main_button(ui: &mut Ui, space: WorkspaceMode, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).strong().color(egui::Color32::WHITE))
            .fill(space.color())
            .corner_radius(8)
            .min_size(Vec2::new(180.0, 30.0)),
    )
}

/// A card of a page: a panel with a small title, as wide as its column.
pub(super) fn card<R>(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::NONE
        .fill(theme::panel_background())
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .corner_radius(10)
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title.to_uppercase()).small().color(theme::text_hint()));
            ui.add_space(theme::PAD / 2.0);
            body(ui)
        })
        .inner
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
