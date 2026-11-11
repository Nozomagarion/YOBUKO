// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a DJ deck does beyond Play and Cue: eight hot cues kept in the DJ library, loops (in,
//! out, reloop, beat loops halved and doubled), beat jumps, the nudge buttons, back to the
//! start, eject and the tempo fader's range. The beats come from the track's beat grid when it
//! has one, else from its tempo.

use super::*;

/// How many hot cues a deck has, and the name a hot cue has in the DJ library.
pub(super) const HOT_CUES: usize = 8;
pub(super) fn hot_cue_name(pad: usize) -> String {
    format!("Hot cue {}", pad + 1)
}
/// The hot cues' colours, one per pad.
pub(super) const PAD_COLORS: [Color32; HOT_CUES] = [
    Color32::from_rgb(0xEF, 0x44, 0x44),
    Color32::from_rgb(0xF5, 0x9E, 0x0B),
    Color32::from_rgb(0xEA, 0xB3, 0x08),
    Color32::from_rgb(0x22, 0xC5, 0x5E),
    Color32::from_rgb(0x06, 0xB6, 0xD4),
    Color32::from_rgb(0x3B, 0x82, 0xF6),
    Color32::from_rgb(0x8B, 0x5C, 0xF6),
    Color32::from_rgb(0xEC, 0x48, 0x99),
];
/// The tempo fader's ranges, in percent, a click on the range moving to the next.
pub(super) const TEMPO_RANGES: [f64; 3] = [8.0, 16.0, 50.0];
/// The beat loop and beat jump sizes the /2 and ×2 buttons go between.
const MIN_BEATS: f64 = 1.0 / 32.0;
const MAX_BEATS: f64 = 64.0;
/// How much faster or slower a nudge button plays the deck while it is held.
pub(super) const NUDGE: f64 = 0.04;
/// How long a jog wheel's turn keeps nudging after its last step.
const JOG_HOLD: Duration = Duration::from_millis(120);

/// What a deck knows beyond the session's reply: kept by the page.
pub(in crate::app) struct DeckTools {
    /// Where LOOP IN was pressed, waiting for LOOP OUT.
    pub(super) loop_in: Option<f64>,
    /// The beat loop's and the beat jump's sizes, in beats.
    pub(super) loop_beats: f64,
    pub(super) jump_beats: f64,
    /// The tempo fader's range, an index into [`TEMPO_RANGES`].
    pub(super) range: usize,
    /// The nudge sent to the deck, and the one a jog wheel asks for until when.
    pub(super) bend: f64,
    pub(super) jog: Option<(f64, Instant)>,
    /// The nudge a button held asks for, set each frame the deck is drawn.
    pub(super) held: f64,
    /// The meter's needle, left and right, falling back slowly after a peak.
    pub(super) meter: [f32; 2],
    /// When TAP was pressed, the last few times.
    pub(super) taps: Vec<Instant>,
}

impl Default for DeckTools {
    fn default() -> Self {
        Self {
            loop_in: None,
            loop_beats: 4.0,
            jump_beats: 4.0,
            range: 0,
            bend: 0.0,
            jog: None,
            held: 0.0,
            meter: [0.0; 2],
            taps: Vec::new(),
        }
    }
}

impl Decks {
    /// The track a deck holds, by its id in the library.
    pub(super) fn track(&self, deck: usize) -> Option<u64> {
        self.deck(deck)["track"].as_u64()
    }

    /// The deck's hot cues: per pad, the cue's id in the library and its place in seconds.
    pub(super) fn hot_cues(&self, deck: usize) -> [Option<(u64, f64)>; HOT_CUES] {
        let mut pads = [None; HOT_CUES];
        let Some(track) = self.track(deck) else { return pads };
        let sets = self.snapshot["cues"].as_array().into_iter().flatten();
        for set in sets.filter(|s| s["track"].as_u64() == Some(track)) {
            for cue in set["points"].as_array().into_iter().flatten() {
                let name = cue["name"].as_str().unwrap_or_default();
                if let Some(pad) = (0..HOT_CUES).find(|p| hot_cue_name(*p) == name)
                    && let (Some(id), Some(seconds)) = (cue["id"].as_u64(), cue["seconds"].as_f64())
                {
                    pads[pad] = Some((id, seconds));
                }
            }
        }
        pads
    }

    /// The deck's beat grid: its first beat in seconds and one beat's length, from the grid
    /// saved for the track or else its tempo.
    pub(super) fn beat(&self, deck: usize) -> Option<(f64, f64)> {
        let track = self.track(deck)?;
        let grid = self.snapshot["grids"].as_array().into_iter().flatten().find(|g| g["track"].as_u64() == Some(track));
        let (first, bpm) = match grid {
            Some(grid) => (grid["first_beat"].as_f64().unwrap_or(0.0), grid["bpm"].as_f64()?),
            None => (0.0, self.deck(deck)["bpm"].as_f64()?),
        };
        (bpm.is_finite() && bpm >= 20.0).then(|| (first, 60.0 / bpm))
    }

    /// The loop as the deck has it: from, to, and whether it plays.
    pub(super) fn current_loop(&self, deck: usize) -> Option<(f64, f64, bool)> {
        let d = self.deck(deck);
        Some((d["loopFrom"].as_f64()?, d["loopTo"].as_f64()?, d["loopOn"] == true))
    }
}

impl App {
    /// A hot cue pad: jumps to its cue, or, when it has none, puts one where the deck is.
    pub(super) fn hot_cue(&mut self, deck: usize, pad: usize) {
        let Some(track) = self.dj.decks.track(deck) else { return };
        match self.dj.decks.hot_cues(deck).get(pad).copied().flatten() {
            Some((_, seconds)) => {
                // Quantized and playing, the jump keeps where in its beat the deck is.
                let here = self.dj.decks.position(deck);
                let offset = match self.dj.decks.nearest_beat(deck, here) {
                    Some(beat) if self.dj.decks.quantize[deck] && self.dj.decks.playing(deck) => here - beat,
                    _ => 0.0,
                };
                self.seek(deck, (seconds + offset).max(0.0));
            }
            None => {
                let revision = self.dj.decks.snapshot["revision"].as_u64().unwrap_or(0);
                let seconds = self.dj.decks.quantized(deck, self.dj.decks.position(deck));
                let params =
                    json!({"track": track, "revision": revision, "name": hot_cue_name(pad), "seconds": seconds});
                self.deck_command("dj.cue.add", params);
            }
        }
    }

    /// Deletes a hot cue.
    pub(super) fn delete_hot_cue(&mut self, deck: usize, pad: usize) {
        let Some(track) = self.dj.decks.track(deck) else { return };
        if let Some((cue, _)) = self.dj.decks.hot_cues(deck).get(pad).copied().flatten() {
            let revision = self.dj.decks.snapshot["revision"].as_u64().unwrap_or(0);
            self.deck_command("dj.cue.delete", json!({"track": track, "revision": revision, "cue": cue}));
        }
    }

    /// Sends a loop, and shows it before the next read.
    fn send_loop(&mut self, deck: usize, region: Option<(f64, f64)>, on: bool) -> bool {
        let mut params = json!({"deck": NAMES[deck], "on": on});
        if let Some((from, to)) = region {
            params["from"] = json!(from);
            params["to"] = json!(to);
        }
        let sent = self.deck_command("dj.deck.loop", params);
        if sent {
            if let Some((from, to)) = region {
                self.dj.decks.set(Some(deck), "loopFrom", json!(from));
                self.dj.decks.set(Some(deck), "loopTo", json!(to));
            }
            self.dj.decks.set(Some(deck), "loopOn", json!(on));
        }
        sent
    }

    /// Lets the loop go, without RELOOP's jump.
    #[cfg(test)]
    pub(super) fn send_loop_off(&mut self, deck: usize) {
        self.send_loop(deck, None, false);
    }

    /// LOOP IN: the loop will start here.
    pub(super) fn loop_in(&mut self, deck: usize) {
        if self.dj.decks.track(deck).is_some() {
            self.dj.decks.tools[deck].loop_in = Some(self.dj.decks.quantized(deck, self.dj.decks.position(deck)));
        }
    }

    /// LOOP OUT: the loop ends here and plays, from LOOP IN or the loop's start.
    pub(super) fn loop_out(&mut self, deck: usize) {
        let here = self.dj.decks.quantized(deck, self.dj.decks.position(deck));
        let from = self.dj.decks.tools[deck].loop_in.or_else(|| self.dj.decks.current_loop(deck).map(|l| l.0));
        let Some(from) = from.filter(|from| here - from >= 0.01) else {
            self.dj.decks.error = Some("Press LOOP IN first, then LOOP OUT a little later.".into());
            return;
        };
        if self.send_loop(deck, Some((from, here)), true) {
            self.dj.decks.tools[deck].loop_in = None;
        }
    }

    /// RELOOP / EXIT: lets a playing loop go, or plays the last one again from its start.
    pub(super) fn reloop(&mut self, deck: usize) {
        match self.dj.decks.current_loop(deck) {
            Some((_, _, true)) => {
                self.send_loop(deck, None, false);
            }
            Some((from, to, false)) => {
                let here = self.dj.decks.position(deck);
                if self.send_loop(deck, None, true) && !(from..to).contains(&here) {
                    self.seek(deck, from);
                }
            }
            None => self.dj.decks.error = Some("No loop yet: press LOOP IN and LOOP OUT, or a beat loop.".into()),
        }
    }

    /// A loop of `beats` beats from the beat nearest to where the deck is; the same loop again
    /// lets it go.
    pub(super) fn beat_loop(&mut self, deck: usize, beats: f64) {
        let Some((first, beat)) = self.dj.decks.beat(deck) else {
            self.dj.decks.error = Some("This track's tempo is not known yet, so beat loops cannot be measured.".into());
            return;
        };
        let length = beats * beat;
        if let Some((from, to, true)) = self.dj.decks.current_loop(deck)
            && ((to - from) - length).abs() < 1e-3
        {
            self.send_loop(deck, None, false);
            return;
        }
        let here = self.dj.decks.position(deck);
        let from = (first + ((here - first) / beat).round() * beat).max(0.0);
        let end = self.dj.decks.number(deck, "seconds", 0.0);
        if from + length > end {
            self.dj.decks.error = Some("That loop would run past the end of the track.".into());
            return;
        }
        self.dj.decks.tools[deck].loop_beats = beats;
        self.send_loop(deck, Some((from, from + length)), true);
    }

    /// /2 or ×2 (`factor` 0.5 or 2): the beat loop's size, and a loop that plays with it.
    pub(super) fn scale_loop(&mut self, deck: usize, factor: f64) {
        let tools = &mut self.dj.decks.tools[deck];
        tools.loop_beats = (tools.loop_beats * factor).clamp(MIN_BEATS, MAX_BEATS);
        if let Some((from, to, on)) = self.dj.decks.current_loop(deck) {
            let end = self.dj.decks.number(deck, "seconds", 0.0);
            let to = (from + (to - from) * factor).min(end);
            if to - from >= 0.01 {
                self.send_loop(deck, Some((from, to)), on);
            }
        }
    }

    /// The beat jump's size, halved or doubled.
    pub(super) fn scale_jump(&mut self, deck: usize, factor: f64) {
        let tools = &mut self.dj.decks.tools[deck];
        tools.jump_beats = (tools.jump_beats * factor).clamp(MIN_BEATS, MAX_BEATS);
    }

    /// Jumps `beats` beats on (back when negative); a loop that plays moves with it.
    pub(super) fn beat_jump(&mut self, deck: usize, beats: f64) {
        let Some((_, beat)) = self.dj.decks.beat(deck) else {
            self.dj.decks.error = Some("This track's tempo is not known yet, so beat jumps cannot be measured.".into());
            return;
        };
        let by = beats * beat;
        if let Some((from, to, true)) = self.dj.decks.current_loop(deck)
            && from + by >= 0.0
            && to + by <= self.dj.decks.number(deck, "seconds", 0.0)
        {
            self.send_loop(deck, Some((from + by, to + by)), true);
        }
        let here = self.dj.decks.position(deck);
        self.seek(deck, here + by);
    }

    /// Back to the start of the track.
    pub(super) fn back_to_start(&mut self, deck: usize) {
        if self.dj.decks.track(deck).is_some() {
            self.seek(deck, 0.0);
        }
    }

    /// Takes the track out of the deck, unless it plays.
    pub(super) fn eject(&mut self, deck: usize) {
        if self.dj.decks.playing(deck) {
            self.dj.decks.error = Some(format!("Deck {} is playing: pause it before ejecting.", NAMES[deck]));
            return;
        }
        if self.deck_command("dj.deck.clear", json!({"deck": NAMES[deck]})) {
            for key in ["track", "title", "seconds", "bpm", "key", "loopFrom", "loopTo"] {
                self.dj.decks.set(Some(deck), key, Value::Null);
            }
            self.dj.decks.tools[deck].loop_in = None;
            self.dj.decks.cue[deck] = 0.0;
        }
    }

    /// A jog wheel turned while the deck plays: a nudge for a moment, more for a faster turn.
    pub(super) fn jog_nudge(&mut self, deck: usize, steps: i32) {
        let amount = (f64::from(steps) * 0.01).clamp(-0.25, 0.25);
        self.dj.decks.tools[deck].jog = Some((amount, Instant::now() + JOG_HOLD));
        self.apply_bend(deck);
    }

    /// Sends the nudge a button held or a jog wheel asks for, when it changed.
    pub(super) fn apply_bend(&mut self, deck: usize) {
        let tools = &mut self.dj.decks.tools[deck];
        let jog = tools.jog.filter(|(_, until)| Instant::now() < *until).map(|(amount, _)| amount);
        if jog.is_none() {
            tools.jog = None;
        }
        let wanted = if tools.held != 0.0 { tools.held } else { jog.unwrap_or(0.0) };
        if (wanted - tools.bend).abs() < 1e-9 || !self.dj.decks.playing(deck) && wanted != 0.0 {
            return;
        }
        if self.deck_command("dj.deck.bend", json!({"deck": NAMES[deck], "amount": wanted})) {
            self.dj.decks.tools[deck].bend = wanted;
        }
    }

    /// The hot cue pads, eight in a row.
    pub(super) fn pads(&mut self, ui: &mut Ui, deck: usize, ready: bool) {
        let cues = self.dj.decks.hot_cues(deck);
        let gap = 6.0;
        let width = ((ui.available_width() - gap * (HOT_CUES as f32 - 1.0)) / HOT_CUES as f32).max(22.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (pad, cue) in cues.iter().enumerate() {
                let label = format!("Hot cue {} {}", pad + 1, NAMES[deck]);
                let response = pad_button(ui, &label, pad, cue.is_some(), Vec2::new(width, 30.0), ready).on_hover_text(
                    if cue.is_some() {
                        "Click: jump here · Right-click: delete"
                    } else {
                        "Click: put a hot cue where the deck is"
                    },
                );
                if response.clicked() {
                    self.hot_cue(deck, pad);
                } else if response.secondary_clicked() {
                    self.delete_hot_cue(deck, pad);
                }
            }
        });
    }

    /// The loop and beat jump buttons.
    pub(super) fn loop_row(&mut self, ui: &mut Ui, deck: usize, ready: bool) {
        let name = NAMES[deck];
        let looped = self.dj.decks.current_loop(deck);
        let on = looped.is_some_and(|l| l.2);
        let waiting = self.dj.decks.tools[deck].loop_in.is_some();
        let beats = self.dj.decks.tools[deck].loop_beats;
        let jump = self.dj.decks.tools[deck].jump_beats;
        let timed = ready && self.dj.decks.beat(deck).is_some();
        let color = deck_color(deck);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if small_button(ui, &format!("Loop in {name}"), "IN", waiting, color, ready).clicked() {
                self.loop_in(deck);
            }
            if small_button(ui, &format!("Loop out {name}"), "OUT", false, color, ready).clicked() {
                self.loop_out(deck);
            }
            let reloop = if on { "EXIT" } else { "RELOOP" };
            if small_button(ui, &format!("Reloop {name}"), reloop, on, color, ready && looped.is_some()).clicked() {
                self.reloop(deck);
            }
            ui.add_space(6.0);
            if small_button(ui, &format!("Halve loop {name}"), "/2", false, color, ready).clicked() {
                self.scale_loop(deck, 0.5);
            }
            let size = format!("LOOP {}", beats_text(beats));
            if small_button(ui, &format!("Beat loop {name}"), &size, on, color, timed)
                .on_hover_text("A loop of this many beats from the nearest beat")
                .clicked()
            {
                self.beat_loop(deck, beats);
            }
            if small_button(ui, &format!("Double loop {name}"), "×2", false, color, ready).clicked() {
                self.scale_loop(deck, 2.0);
            }
            ui.add_space(6.0);
            if small_button(ui, &format!("Jump back {name}"), "◀", false, color, timed).clicked() {
                self.beat_jump(deck, -jump);
            }
            let size = format!("JUMP {}", beats_text(jump));
            let size_button = small_button(ui, &format!("Beat jump size {name}"), &size, false, color, ready)
                .on_hover_text("Click: double · Right-click: halve");
            if size_button.clicked() {
                self.scale_jump(deck, 2.0);
            } else if size_button.secondary_clicked() {
                self.scale_jump(deck, 0.5);
            }
            if small_button(ui, &format!("Jump forward {name}"), "▶", false, color, timed).clicked() {
                self.beat_jump(deck, jump);
            }
        });
    }
}

/// A number of beats as a DJ says it: 1/4, 1/2, 1, 2, 4 … 64.
pub(super) fn beats_text(beats: f64) -> String {
    if beats >= 1.0 { format!("{beats:.0}") } else { format!("1/{:.0}", 1.0 / beats) }
}

/// A small rounded button, lit when `on`; `label` names it for screen readers and tests.
pub(super) fn small_button(
    ui: &mut Ui,
    label: &str,
    text: &str,
    on: bool,
    color: Color32,
    enabled: bool,
) -> egui::Response {
    let font = FontId::proportional(11.0);
    let width = ui.painter().layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE).size().x + 14.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width.max(28.0), 24.0), if enabled { Sense::click() } else { Sense::hover() });
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, label));
    let hover = response.hovered() && enabled;
    let fill = match (on, hover) {
        (true, _) => color,
        (false, true) => theme::control_hover(),
        (false, false) => theme::control_background(),
    };
    let fill = if enabled { fill } else { fill.gamma_multiply(0.5) };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(8), fill);
    let ink = if on { Color32::WHITE } else { theme::text_primary() };
    let ink = if enabled { ink } else { ink.gamma_multiply(0.45) };
    painter.text(rect.center(), Align2::CENTER_CENTER, text, font, ink);
    response
}

/// A hot cue pad: its colour when the cue is set, an outline when not.
fn pad_button(ui: &mut Ui, label: &str, pad: usize, set: bool, size: Vec2, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, set, label));
    let color = PAD_COLORS[pad % HOT_CUES];
    let hover = response.hovered() && enabled;
    let painter = ui.painter();
    let radius = CornerRadius::same(8);
    if set {
        let face = if hover { crate::kit::motion::mix(color, Color32::WHITE, 0.15) } else { color };
        painter.rect_filled(rect.translate(Vec2::new(0.0, 2.0)), radius, Color32::from_black_alpha(50));
        painter.rect_filled(rect, radius, face);
    } else {
        let face = if hover { theme::control_hover() } else { theme::control_background() };
        painter.rect_filled(rect, radius, if enabled { face } else { face.gamma_multiply(0.5) });
        painter.rect_stroke(rect, radius, Stroke::new(1.5, color.gamma_multiply(0.6)), StrokeKind::Inside);
    }
    let ink = if set { Color32::WHITE } else { theme::text_hint() };
    painter.text(rect.center(), Align2::CENTER_CENTER, (pad + 1).to_string(), FontId::proportional(13.0), ink);
    response
}
