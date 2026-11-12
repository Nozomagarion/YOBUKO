// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Auto DJ and the intro and outro marks. The library's Auto DJ list is the queue the decks
//! play on their own (`dj.autodj.*`); its bar switches it on, skips to the next track and sets
//! the fade. A deck's waveform menu puts the track's intro or outro at the playhead: cues named
//! "Intro" and "Outro", which Auto DJ fades between.

use super::tools::small_button;
use super::*;

/// The cue names Auto DJ reads.
pub(super) const INTRO: &str = "Intro";
pub(super) const OUTRO: &str = "Outro";
/// Auto DJ's colour.
const AUTO_COLOR: Color32 = Color32::from_rgb(0x38, 0xBD, 0xF8);

impl Decks {
    /// What the session says of Auto DJ.
    pub(super) fn auto(&self) -> &Value {
        &self.snapshot["performance"]["autoDj"]
    }

    /// The tracks queued for Auto DJ, by id.
    pub(super) fn auto_queue(&self) -> Vec<u64> {
        self.auto()["queue"].as_array().into_iter().flatten().filter_map(Value::as_u64).collect()
    }

    /// The cue named `name` of the track on deck `deck`: its id and place in seconds.
    pub(super) fn named_cue(&self, deck: usize, name: &str) -> Option<(u64, f64)> {
        let track = self.track(deck)?;
        let sets = self.snapshot["cues"].as_array().into_iter().flatten();
        sets.filter(|s| s["track"].as_u64() == Some(track))
            .flat_map(|s| s["points"].as_array().cloned().unwrap_or_default())
            .find(|c| c["name"] == name)
            .and_then(|c| Some((c["id"].as_u64()?, c["seconds"].as_f64()?)))
    }
}

impl App {
    /// Puts the cue named `name` (the intro or the outro) at deck `deck`'s playhead.
    pub(super) fn set_named_cue(&mut self, deck: usize, name: &str) {
        let Some(track) = self.dj.decks.track(deck) else { return };
        let seconds = self.dj.decks.position(deck);
        let revision = self.dj.decks.snapshot["revision"].as_u64().unwrap_or(0);
        match self.dj.decks.named_cue(deck, name) {
            Some((cue, _)) => self.deck_command(
                "dj.cue.move",
                json!({"track": track, "revision": revision, "cue": cue, "seconds": seconds}),
            ),
            None => self.deck_command(
                "dj.cue.add",
                json!({"track": track, "revision": revision, "name": name, "seconds": seconds}),
            ),
        };
    }

    /// Takes the cue named `name` off deck `deck`'s track.
    pub(super) fn clear_named_cue(&mut self, deck: usize, name: &str) {
        let Some(track) = self.dj.decks.track(deck) else { return };
        if let Some((cue, _)) = self.dj.decks.named_cue(deck, name) {
            let revision = self.dj.decks.snapshot["revision"].as_u64().unwrap_or(0);
            self.deck_command("dj.cue.delete", json!({"track": track, "revision": revision, "cue": cue}));
        }
    }

    /// The menu of a deck's waveform: its intro and outro.
    pub(super) fn marks_menu(&mut self, response: &egui::Response, deck: usize) {
        let mut action = None;
        response.context_menu(|ui| {
            for name in [INTRO, OUTRO] {
                let lower = name.to_lowercase();
                if ui.button(format!("Set the {lower} at the playhead")).clicked() {
                    action = Some((name, true));
                    ui.close();
                }
                let there = self.dj.decks.named_cue(deck, name).is_some();
                if ui.add_enabled(there, egui::Button::new(format!("Clear the {lower}"))).clicked() {
                    action = Some((name, false));
                    ui.close();
                }
            }
        });
        match action {
            Some((name, true)) => self.set_named_cue(deck, name),
            Some((name, false)) => self.clear_named_cue(deck, name),
            None => {}
        }
    }

    /// Switches Auto DJ on or off.
    pub(super) fn toggle_auto_dj(&mut self) {
        let on = self.dj.decks.auto()["on"] == true;
        if self.deck_command("dj.autodj.set", json!({"on": !on})) {
            self.dj.decks.snapshot["performance"]["autoDj"]["on"] = json!(!on);
        }
    }

    /// Queues library track `track` for Auto DJ.
    pub(super) fn queue_auto_dj(&mut self, track: u64) {
        if self.deck_command("dj.autodj.add", json!({"track": track}))
            && let Some(queue) = self.dj.decks.snapshot["performance"]["autoDj"]["queue"].as_array_mut()
        {
            queue.push(json!(track));
        }
    }

    /// Auto DJ's buttons: on, skip, and the fade's length.
    pub(super) fn auto_dj_bar(&mut self, ui: &mut Ui) {
        let auto = self.dj.decks.auto().clone();
        let on = auto["on"] == true;
        let fade = auto["fade"].as_f64().unwrap_or(8.0);
        ui.label(egui::RichText::new("AUTO DJ").small().color(theme::text_hint()));
        let queued = auto["queue"].as_array().map_or(0, Vec::len);
        if small_button(ui, "Auto DJ", if on { "ON" } else { "OFF" }, on, AUTO_COLOR, on || queued > 0)
            .on_hover_text("The decks play the Auto DJ list one after the other, fading from each track's outro")
            .on_disabled_hover_text("Add tracks to the Auto DJ list first: right-click a track")
            .clicked()
        {
            self.toggle_auto_dj();
        }
        if small_button(ui, "Skip to the next track", "SKIP", false, AUTO_COLOR, on).clicked() {
            self.deck_command("dj.autodj.skip", json!({}));
        }
        for (label, text, by) in [("Shorter fade", "◀", -1.0), ("Longer fade", "▶", 1.0)] {
            if by > 0.0 {
                ui.label(egui::RichText::new(format!("FADE {fade:.0} s")).small().color(theme::text_primary()));
            }
            if small_button(ui, label, text, false, AUTO_COLOR, true).clicked() {
                let next = (fade + by).clamp(0.0, 30.0);
                if self.deck_command("dj.autodj.set", json!({"fade": next})) {
                    self.dj.decks.snapshot["performance"]["autoDj"]["fade"] = json!(next);
                }
            }
        }
        if let Some(e) = auto["error"].as_str() {
            ui.colored_label(theme::recording(), e);
        }
    }
}
