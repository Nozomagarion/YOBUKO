// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ sampler: eight slots in a row, each holding a library track that a pad plays from its
//! start into the master, with a level. A slot is loaded from the library (right-click a track,
//! or right-click the slot for the library's selected track); the decks' pads play the slots
//! in SAMPLER mode. It drives the session's `dj.sampler.*` commands.

use super::tools::small_button;
use super::*;

/// The sampler's slots.
pub(super) const SLOTS: usize = 8;
/// The sampler's card height.
pub(super) const SAMPLER_HEIGHT: f32 = 92.0;
/// The slots' colour.
const SAMPLER_COLOR: Color32 = Color32::from_rgb(0xF5, 0x9E, 0x0B);

impl Decks {
    /// What the session says of sampler slot `slot` (0 … 7).
    pub(super) fn sampler(&self, slot: usize) -> &Value {
        &self.snapshot["performance"]["samplers"][slot]
    }
}

impl App {
    /// Plays a slot from its start, or loads the library's selected track into an empty one.
    pub(super) fn sampler_pad(&mut self, slot: usize) {
        if self.dj.decks.sampler(slot)["track"].is_number() {
            if self.deck_command("dj.sampler.play", json!({"slot": slot + 1})) {
                self.dj.decks.snapshot["performance"]["samplers"][slot]["playing"] = json!(true);
            }
        } else {
            self.load_sampler_selected(slot);
        }
    }

    pub(super) fn stop_sampler(&mut self, slot: usize) {
        if self.deck_command("dj.sampler.stop", json!({"slot": slot + 1})) {
            self.dj.decks.snapshot["performance"]["samplers"][slot]["playing"] = json!(false);
        }
    }

    /// Loads library track `track` into a slot.
    pub(super) fn load_sampler(&mut self, slot: usize, track: u64) {
        if self.deck_command("dj.sampler.load", json!({"slot": slot + 1, "track": track})) {
            self.dj.decks.snapshot["performance"]["samplers"][slot]["loading"] = json!(true);
        }
    }

    fn load_sampler_selected(&mut self, slot: usize) {
        let shown = self.dj.decks.shown_tracks();
        if let Some(id) = shown.get(self.dj.decks.selected).and_then(|t| t["id"].as_u64()) {
            self.load_sampler(slot, id);
        }
    }

    /// The sampler's eight slots in a row.
    pub(super) fn sampler_row(&mut self, ui: &mut Ui) {
        egui::Frame::NONE
            .fill(theme::panel_background())
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(CARD_RADIUS)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(SAMPLER_HEIGHT - 20.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("SAMPLER").small().color(theme::text_hint()));
                    let gap = 8.0;
                    let width = ((ui.available_width() - gap * (SLOTS as f32 - 1.0)) / SLOTS as f32).max(80.0);
                    ui.spacing_mut().item_spacing.x = gap;
                    for slot in 0..SLOTS {
                        ui.allocate_ui_with_layout(
                            Vec2::new(width, SAMPLER_HEIGHT - 20.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| self.sampler_slot(ui, slot, width),
                        );
                    }
                });
            });
    }

    fn sampler_slot(&mut self, ui: &mut Ui, slot: usize, width: f32) {
        let s = self.dj.decks.sampler(slot).clone();
        let loaded = s["track"].is_number();
        let playing = s["playing"] == true;
        let loading = s["loading"] == true;
        let title = if loading {
            "Loading…".to_owned()
        } else {
            s["title"].as_str().map_or_else(|| "Empty".to_owned(), str::to_owned)
        };
        let label = format!("Sampler {}", slot + 1);
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 40.0), Sense::click());
        response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, playing, &label));
        let hover = response.hovered();
        let fill = match (playing, loaded, hover) {
            (true, _, _) => SAMPLER_COLOR,
            (false, true, true) => theme::control_hover(),
            (false, true, false) => theme::control_background(),
            (false, false, _) => Color32::TRANSPARENT,
        };
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(10), fill);
        if !loaded {
            painter.rect_stroke(
                rect.shrink(1.0),
                CornerRadius::same(10),
                Stroke::new(1.5, theme::border()),
                StrokeKind::Inside,
            );
        }
        let ink = if playing {
            Color32::WHITE
        } else if loaded {
            theme::text_primary()
        } else {
            theme::text_hint()
        };
        painter.text(
            rect.left_top() + Vec2::new(8.0, 6.0),
            Align2::LEFT_TOP,
            format!("{}", slot + 1),
            FontId::proportional(10.0),
            ink,
        );
        let galley = painter.layout(title, FontId::proportional(11.0), ink, rect.width() - 16.0);
        painter.galley(Pos2::new(rect.left() + 8.0, rect.bottom() - 6.0 - galley.size().y.min(26.0)), galley, ink);
        let response = response.on_hover_text(if loaded {
            "Click: play from the start · Right-click: stop, load or empty"
        } else {
            "Click: load the library's selected track · Right-click: more"
        });
        if response.clicked() {
            self.sampler_pad(slot);
        }
        response.context_menu(|ui| {
            if ui.add_enabled(playing, egui::Button::new("Stop")).clicked() {
                self.stop_sampler(slot);
                ui.close();
            }
            if ui.button("Load the library's selected track").clicked() {
                self.load_sampler_selected(slot);
                ui.close();
            }
            if ui.add_enabled(loaded, egui::Button::new("Empty this slot")).clicked() {
                self.deck_command("dj.sampler.clear", json!({"slot": slot + 1}));
                ui.close();
            }
        });
        ui.add_space(4.0);
        let mut gain = s["gain"].as_f64().unwrap_or(1.0) as f32;
        let level = format!("Sampler {} level", slot + 1);
        ui.spacing_mut().slider_width = width;
        let slider = egui::Slider::new(&mut gain, 0.0..=1.0).show_value(false).trailing_fill(true);
        let response = ui.add(slider);
        response.widget_info(|| egui::WidgetInfo::slider(true, f64::from(gain), &level));
        if response.changed()
            && self.deck_command("dj.sampler.gain", json!({"slot": slot + 1, "gain": f64::from(gain)}))
        {
            self.dj.decks.snapshot["performance"]["samplers"][slot]["gain"] = json!(gain);
        }
    }

    /// A deck's pad mode button: hot cues, or the sampler's slots.
    pub(super) fn pad_mode(&mut self, ui: &mut Ui, deck: usize) {
        let sampler = self.dj.decks.sampler_pads[deck];
        let color = deck_color(deck);
        let text = if sampler { "SAMPLER" } else { "HOT CUE" };
        if small_button(ui, &format!("Pad mode {}", NAMES[deck]), text, sampler, color, true)
            .on_hover_text("What the eight pads play: the hot cues, or the sampler's slots")
            .clicked()
        {
            self.dj.decks.sampler_pads[deck] = !sampler;
        }
    }

    /// The pads in SAMPLER mode: one per slot, lit while it plays.
    pub(super) fn sampler_pads(&mut self, ui: &mut Ui, deck: usize) {
        let gap = 6.0;
        let width = ((ui.available_width() - gap * (SLOTS as f32 - 1.0)) / SLOTS as f32).max(22.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for slot in 0..SLOTS {
                let s = self.dj.decks.sampler(slot);
                let (loaded, playing) = (s["track"].is_number(), s["playing"] == true);
                let label = format!("Sampler pad {} {}", slot + 1, NAMES[deck]);
                let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 30.0), Sense::click());
                response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, playing, &label));
                let painter = ui.painter();
                if playing {
                    painter.rect_filled(rect, CornerRadius::same(8), SAMPLER_COLOR);
                } else if loaded {
                    painter.rect_filled(rect, CornerRadius::same(8), SAMPLER_COLOR.gamma_multiply(0.35));
                } else {
                    let stroke = Stroke::new(1.5, SAMPLER_COLOR.gamma_multiply(0.6));
                    painter.rect_stroke(rect.shrink(1.0), CornerRadius::same(8), stroke, StrokeKind::Inside);
                }
                let ink = if loaded { Color32::WHITE } else { theme::text_hint() };
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    format!("S{}", slot + 1),
                    FontId::proportional(12.0),
                    ink,
                );
                let response = response.on_hover_text(if loaded {
                    "Click: play this sampler slot from its start · Right-click: stop it"
                } else {
                    "Click: load the library's selected track into this slot"
                });
                if response.clicked() {
                    self.sampler_pad(slot);
                } else if response.secondary_clicked() {
                    self.stop_sampler(slot);
                }
            }
        });
    }
}
