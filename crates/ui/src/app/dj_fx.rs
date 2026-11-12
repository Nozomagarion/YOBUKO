// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The DJ space's effects: two effect units under the decks, each with three effects (chosen
//! from a list, switched on, and turned with one knob), a mix knob, a length in beats, and the
//! places it goes (decks A–D, the master). They drive the session's `dj.fx.set`; the
//! units' tempo follows the deck they are on.

use super::tools::small_button;
use super::*;

/// The lengths the ◀ and ▶ buttons step through, in beats.
const BEATS: [f64; 10] = [1.0 / 16.0, 1.0 / 8.0, 1.0 / 4.0, 1.0 / 2.0, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
/// The units' card height.
pub(super) const FX_HEIGHT: f32 = 104.0;

impl Decks {
    /// What the session says of effect unit `unit` (0 or 1).
    pub(super) fn fx(&self, unit: usize) -> &Value {
        &self.snapshot["performance"]["fx"][unit]
    }

    /// The effects there are: their ids and names.
    fn effects(&self) -> Vec<(String, String)> {
        self.snapshot["performance"]["effects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| Some((e["id"].as_str()?.to_owned(), e["name"].as_str()?.to_owned())))
            .collect()
    }

    /// The tempo unit `unit` should count beats at: the first deck it is on that has one, else
    /// a playing deck's.
    fn fx_tempo(&self, unit: usize) -> Option<f64> {
        let fx = self.fx(unit);
        let on: Vec<usize> =
            [("deckA", 0), ("deckB", 1)].into_iter().filter(|(k, _)| fx[*k] == true).map(|(_, d)| d).collect();
        on.iter()
            .chain([0, 1].iter().filter(|d| self.playing(**d)))
            .find_map(|d| self.bpm(*d))
            .filter(|bpm| (20.0..=400.0).contains(bpm))
    }
}

impl App {
    /// Sends a change to effect unit `unit`, and shows it before the next read.
    pub(super) fn set_fx(&mut self, unit: usize, change: Value) -> bool {
        let mut params = change.clone();
        params["unit"] = json!(unit + 1);
        if !self.deck_command("dj.fx.set", params) {
            return false;
        }
        let fx = &mut self.dj.decks.snapshot["performance"]["fx"][unit];
        if fx.is_object() {
            let slot = change["slot"].as_u64().and_then(|s| usize::try_from(s).ok()).map(|s| s.saturating_sub(1));
            if let Some(map) = change.as_object() {
                for (key, value) in map {
                    match (key.as_str(), slot) {
                        ("unit" | "slot", _) => {}
                        ("effect", Some(s)) => {
                            let effect = if value == "none" { Value::Null } else { value.clone() };
                            fx["slots"][s]["effect"] = effect;
                        }
                        ("on" | "meta", Some(s)) => fx["slots"][s][key.as_str()] = value.clone(),
                        _ => fx[key.as_str()] = value.clone(),
                    }
                }
            }
        }
        true
    }

    /// Keeps each unit's tempo with the deck it is on.
    fn follow_fx_tempo(&mut self) {
        for unit in 0..2 {
            let Some(bpm) = self.dj.decks.fx_tempo(unit) else { continue };
            let sent = self.dj.decks.fx(unit)["bpm"].as_f64().unwrap_or(0.0);
            if self.dj.decks.fx(unit).is_object() && (bpm - sent).abs() > 0.05 {
                self.set_fx(unit, json!({"bpm": bpm}));
            }
        }
    }

    /// Switches a slot's effect on or off; one with no effect stays off.
    pub(super) fn toggle_fx(&mut self, unit: usize, slot: usize) {
        let s = &self.dj.decks.fx(unit)["slots"][slot];
        if s["effect"].is_string() {
            let on = s["on"] == true;
            self.set_fx(unit, json!({"slot": slot + 1, "on": !on}));
        }
    }

    /// Picks the next (or, with `by` below 0, the previous) effect for a slot.
    pub(super) fn step_fx(&mut self, unit: usize, slot: usize, by: i32) {
        let effects = self.dj.decks.effects();
        if effects.is_empty() {
            return;
        }
        let current = self.dj.decks.fx(unit)["slots"][slot]["effect"].as_str().map(str::to_owned);
        let at = current.and_then(|c| effects.iter().position(|(id, _)| *id == c));
        let count = effects.len() as i32;
        let next = match at {
            Some(i) => (i as i32 + by).rem_euclid(count),
            None if by >= 0 => 0,
            None => count - 1,
        };
        if let Some((id, _)) = effects.get(next as usize) {
            self.set_fx(unit, json!({"slot": slot + 1, "effect": id}));
        }
    }

    /// The length in beats one step shorter (below 0) or longer.
    pub(super) fn step_fx_beats(&mut self, unit: usize, by: i32) {
        let beats = self.dj.decks.fx(unit)["beats"].as_f64().unwrap_or(1.0);
        let at = BEATS.iter().position(|b| (b - beats).abs() < 1e-6).unwrap_or(4) as i32;
        let next = (at + by).clamp(0, BEATS.len() as i32 - 1) as usize;
        self.set_fx(unit, json!({"beats": BEATS[next]}));
    }

    /// The two effect units, side by side.
    pub(super) fn fx_row(&mut self, ui: &mut Ui) {
        self.follow_fx_tempo();
        let width = ((ui.available_width() - GAP) / 2.0).max(300.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;
            let down = egui::Layout::top_down(egui::Align::Min);
            for unit in 0..2 {
                ui.allocate_ui_with_layout(Vec2::new(width, FX_HEIGHT), down, |ui| self.fx_card(ui, unit));
            }
        });
    }

    fn fx_card(&mut self, ui: &mut Ui, unit: usize) {
        let color = deck_color(unit);
        let fx = self.dj.decks.fx(unit).clone();
        let ready = fx.is_object();
        let effects = self.dj.decks.effects();
        let name = format!("FX {}", unit + 1);
        egui::Frame::NONE
            .fill(theme::panel_background())
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(CARD_RADIUS)
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(FX_HEIGHT - 20.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&name).strong().color(color));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            let places = [
                                ("deckA", "A", "deck A"),
                                ("deckB", "B", "deck B"),
                                ("deckC", "C", "deck C"),
                                ("deckD", "D", "deck D"),
                                ("master", "MST", "the master"),
                            ];
                            let four = self.dj.decks.four;
                            for (key, text, place) in
                                places.into_iter().filter(|(k, ..)| four || !matches!(*k, "deckC" | "deckD"))
                            {
                                let on = fx[key] == true;
                                if small_button(ui, &format!("{name} on {place}"), text, on, color, ready)
                                    .on_hover_text(format!("Put {name} on {place}"))
                                    .clicked()
                                {
                                    let mut change = json!({});
                                    change[key] = json!(!on);
                                    self.set_fx(unit, change);
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            let beats = fx["beats"].as_f64().unwrap_or(1.0);
                            if small_button(ui, &format!("{name} shorter"), "◀", false, color, ready).clicked() {
                                self.step_fx_beats(unit, -1);
                            }
                            let text = format!("{} BEAT", tools::beats_text(beats));
                            small_button(ui, &format!("{name} beats"), &text, false, color, ready)
                                .on_hover_text("The length the echoes and sweeps follow, at the deck's tempo");
                            if small_button(ui, &format!("{name} longer"), "▶", false, color, ready).clicked() {
                                self.step_fx_beats(unit, 1);
                            }
                        });
                    });
                    for slot in 0..3 {
                        ui.separator();
                        self.fx_slot(ui, unit, slot, &fx, &effects);
                    }
                    ui.separator();
                    let mut mix = fx["mix"].as_f64().unwrap_or(0.5) as f32;
                    if dj_knob(ui, &format!("{name} mix"), "Mix", &mut mix, 0.5, color, false) {
                        self.set_fx(unit, json!({"mix": f64::from(mix)}));
                    }
                });
            });
    }

    fn fx_slot(&mut self, ui: &mut Ui, unit: usize, slot: usize, fx: &Value, effects: &[(String, String)]) {
        let color = deck_color(unit);
        let s = &fx["slots"][slot];
        let effect = s["effect"].as_str();
        let shown = effect
            .and_then(|id| effects.iter().find(|(e, _)| e == id))
            .map_or("—".to_owned(), |(_, name)| name.clone());
        let on = s["on"] == true;
        let label = format!("FX {} effect {}", unit + 1, slot + 1);
        ui.vertical(|ui| {
            ui.set_width(104.0);
            let pick = small_button(ui, &label, &shown, false, color, fx.is_object())
                .on_hover_text("Choose the effect · Right-click: next one");
            if pick.secondary_clicked() {
                self.step_fx(unit, slot, 1);
            }
            egui::Popup::menu(&pick).show(|ui| {
                if ui.button("None").clicked() {
                    self.set_fx(unit, json!({"slot": slot + 1, "effect": "none"}));
                }
                for (id, name) in effects {
                    if ui.selectable_label(effect == Some(id.as_str()), name).clicked() {
                        self.set_fx(unit, json!({"slot": slot + 1, "effect": id}));
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let switch = format!("FX {} on {}", unit + 1, slot + 1);
                if small_button(ui, &switch, "ON", on, color, effect.is_some())
                    .on_hover_text("Switch this effect on or off")
                    .clicked()
                {
                    self.toggle_fx(unit, slot);
                }
                let mut meta = s["meta"].as_f64().unwrap_or(0.5) as f32;
                let knob = format!("FX {} knob {}", unit + 1, slot + 1);
                if dj_knob(ui, &knob, "Amount", &mut meta, 0.5, color, false) {
                    self.set_fx(unit, json!({"slot": slot + 1, "meta": f64::from(meta)}));
                }
            });
        });
    }
}
