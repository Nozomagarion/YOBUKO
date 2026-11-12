// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A DJ deck's key and beats: the key lock, the key moved up or down or matched to the other
//! deck, quantize (cues and loops land on the beat, jumps keep the beat), Sync that also lines the
//! beats up, tapping the tempo, and moving the beat grid.

use super::tools::small_button;
use super::*;

/// The note names a key is shown with, from C.
const NOTES: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
/// Taps further apart than this start a new count.
const TAP_GAP: Duration = Duration::from_millis(2000);
/// Taps kept for the average.
const TAPS: usize = 8;
/// How far one click of the grid's arrows moves it, in seconds.
const GRID_STEP: f64 = 0.005;

/// A key: its tonic (0 for C … 11 for B), whether it is minor, and whether it was written as a
/// Camelot code ("8A"), to be shown the same way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Key {
    pub(super) tonic: u8,
    pub(super) minor: bool,
    pub(super) camelot: bool,
}

impl Key {
    /// Reads "Am", "C#", "Bbm", "F# minor", "A major" or a Camelot code ("8A", "12B").
    pub(super) fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            let number: u8 = digits.parse().ok().filter(|n| (1..=12).contains(n))?;
            let minor = match text.get(digits.len()..)?.trim() {
                "A" | "a" => true,
                "B" | "b" => false,
                _ => return None,
            };
            // 8B is C major; each step round the wheel is a fifth (7 semitones); A is the
            // relative minor, three semitones below.
            let major = (u16::from(number) + 4) * 7 % 12;
            let tonic = if minor { (major + 9) % 12 } else { major };
            return Some(Self { tonic: tonic as u8, minor, camelot: true });
        }
        let mut chars = text.chars();
        let letter = chars.next()?.to_ascii_uppercase();
        let mut tonic: i32 = match letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let rest: String = chars.collect();
        let mut rest = rest.as_str();
        if let Some(r) = rest.strip_prefix('#').or_else(|| rest.strip_prefix('♯')) {
            tonic += 1;
            rest = r;
        } else if let Some(r) = rest.strip_prefix('b').or_else(|| rest.strip_prefix('♭')) {
            tonic -= 1;
            rest = r;
        }
        let rest = rest.trim().to_ascii_lowercase();
        let minor = match rest.as_str() {
            "m" | "min" | "minor" => true,
            "" | "maj" | "major" => false,
            _ => return None,
        };
        Some(Self { tonic: tonic.rem_euclid(12) as u8, minor, camelot: false })
    }

    /// The key moved by `semitones`.
    pub(super) fn moved(self, semitones: i32) -> Self {
        Self { tonic: (i32::from(self.tonic) + semitones).rem_euclid(12) as u8, ..self }
    }

    /// Written as it was read: "Am" or "8A".
    pub(super) fn text(self) -> String {
        if self.camelot {
            let major = if self.minor { (self.tonic + 3) % 12 } else { self.tonic };
            // The inverse of the step in `parse`: 7 × 7 = 49 ≡ 1 (mod 12).
            let number = (u16::from(major) * 7 + 8 - 1) % 12 + 1;
            format!("{number}{}", if self.minor { "A" } else { "B" })
        } else {
            format!("{}{}", NOTES[usize::from(self.tonic)], if self.minor { "m" } else { "" })
        }
    }

    /// The tonic of the relative major, so a minor key and its relative major count as one.
    fn major_tonic(self) -> i32 {
        i32::from(if self.minor { (self.tonic + 3) % 12 } else { self.tonic })
    }
}

impl Decks {
    /// How far the deck's key is moved, in semitones: the key knob, and the speed too when the
    /// key is not locked.
    pub(super) fn key_moved(&self, deck: usize) -> f64 {
        let d = self.deck(deck);
        let shift = d["keyShift"].as_f64().unwrap_or(0.0);
        let rate = d["rate"].as_f64().unwrap_or(1.0).max(0.01);
        if d["keyLock"] == true { shift } else { shift + 12.0 * rate.log2() }
    }

    /// The deck's key as it sounds now.
    pub(super) fn shown_key(&self, deck: usize) -> Option<Key> {
        let key = Key::parse(self.deck(deck)["key"].as_str()?)?;
        Some(key.moved(self.key_moved(deck).round() as i32))
    }

    /// Where the deck's nearest beat is to `seconds`, when its beats are known.
    pub(super) fn nearest_beat(&self, deck: usize, seconds: f64) -> Option<f64> {
        let (first, beat) = self.beat(deck)?;
        Some((first + ((seconds - first) / beat).round() * beat).max(0.0))
    }

    /// `seconds` on the nearest beat when the deck quantizes, else as it is.
    pub(super) fn quantized(&self, deck: usize, seconds: f64) -> f64 {
        if self.quantize[deck] { self.nearest_beat(deck, seconds).unwrap_or(seconds) } else { seconds }
    }

    /// How far into its beat the deck is, 0 … 1.
    fn phase(&self, deck: usize) -> Option<f64> {
        let (first, beat) = self.beat(deck)?;
        Some(((self.position(deck) - first) / beat).rem_euclid(1.0))
    }
}

impl App {
    /// Sends a deck's key lock and key move, and shows them before the next read.
    fn send_key(&mut self, deck: usize, lock: bool, shift: f64) {
        let shift = shift.clamp(-12.0, 12.0);
        if self.deck_command("dj.deck.key", json!({"deck": NAMES[deck], "lock": lock, "shift": shift})) {
            self.dj.decks.set(Some(deck), "keyLock", json!(lock));
            self.dj.decks.set(Some(deck), "keyShift", json!(shift));
        }
    }

    pub(super) fn toggle_key_lock(&mut self, deck: usize) {
        let d = self.dj.decks.deck(deck);
        let (lock, shift) = (d["keyLock"] == true, d["keyShift"].as_f64().unwrap_or(0.0));
        self.send_key(deck, !lock, shift);
    }

    /// Moves the key up or down a semitone; 0 puts it back where the track has it.
    pub(super) fn move_key(&mut self, deck: usize, by: i32) {
        let d = self.dj.decks.deck(deck);
        let (lock, shift) = (d["keyLock"] == true, d["keyShift"].as_f64().unwrap_or(0.0));
        let shift = if by == 0 { 0.0 } else { shift.round() + f64::from(by) };
        self.send_key(deck, lock, shift);
    }

    /// Moves the deck's key to the other deck's (or its relative major or minor), the shortest
    /// way, and locks it so a tempo change keeps it.
    pub(super) fn key_sync(&mut self, deck: usize) {
        let own = Key::parse(self.dj.decks.deck(deck)["key"].as_str().unwrap_or_default());
        let (Some(own), Some(target)) = (own, self.dj.decks.shown_key(self.dj.decks.partner(deck))) else {
            self.dj.decks.error = Some("Both decks need a track with a known key to match keys.".into());
            return;
        };
        let by = (target.major_tonic() - own.major_tonic() + 6).rem_euclid(12) - 6;
        self.send_key(deck, true, f64::from(by));
    }

    pub(super) fn toggle_quantize(&mut self, deck: usize) {
        self.dj.decks.quantize[deck] = !self.dj.decks.quantize[deck];
    }

    /// Lines the deck's beats up with the other deck's, by the shortest move, when both play
    /// with known beats.
    pub(super) fn align_beats(&mut self, deck: usize) {
        let decks = &self.dj.decks;
        let other = decks.partner(deck);
        if !decks.playing(deck) || !decks.playing(other) {
            return;
        }
        let (Some(own), Some(target), Some((_, beat))) = (decks.phase(deck), decks.phase(other), decks.beat(deck))
        else {
            return;
        };
        let by = ((target - own + 0.5).rem_euclid(1.0) - 0.5) * beat;
        if by.abs() > 1e-4 {
            self.deck_command("dj.deck.seek", json!({"deck": NAMES[deck], "by": by}));
        }
    }

    /// A tap on TAP: after a few, the track's tempo is what the taps say (at the deck's speed),
    /// saved in its beat grid.
    pub(super) fn tap(&mut self, deck: usize) {
        self.tap_at(deck, Instant::now());
    }

    /// A tap at `now`.
    pub(super) fn tap_at(&mut self, deck: usize, now: Instant) {
        let taps = &mut self.dj.decks.tools[deck].taps;
        if taps.last().is_some_and(|last| now.duration_since(*last) > TAP_GAP) {
            taps.clear();
        }
        taps.push(now);
        if taps.len() > TAPS {
            taps.remove(0);
        }
        if taps.len() < 4 {
            return;
        }
        let (Some(first), Some(last)) = (taps.first(), taps.last()) else { return };
        let beat = last.duration_since(*first).as_secs_f64() / (taps.len() - 1) as f64;
        let rate = self.dj.decks.number(deck, "rate", 1.0).max(0.01);
        let bpm = (60.0 / beat.max(1e-3) / rate).clamp(20.0, 400.0);
        let first = self.dj.decks.beat(deck).map_or(0.0, |(first, _)| first);
        self.save_grid(deck, bpm, first);
    }

    /// The beat grid's first beat put where the deck is now (moved back by whole beats).
    pub(super) fn grid_here(&mut self, deck: usize) {
        let Some((_, beat)) = self.dj.decks.beat(deck) else { return };
        let here = self.dj.decks.position(deck);
        self.save_grid(deck, 60.0 / beat, here.rem_euclid(beat));
    }

    /// Moves the beat grid earlier (below 0) or later by `steps` small steps.
    pub(super) fn move_grid(&mut self, deck: usize, steps: f64) {
        let Some((first, beat)) = self.dj.decks.beat(deck) else { return };
        self.save_grid(deck, 60.0 / beat, (first + steps * GRID_STEP).rem_euclid(beat));
    }

    /// The beat grid's tempo up or down by `by` BPM.
    pub(super) fn grid_tempo(&mut self, deck: usize, by: f64) {
        let Some((first, beat)) = self.dj.decks.beat(deck) else { return };
        self.save_grid(deck, (60.0 / beat + by).clamp(20.0, 400.0), first);
    }

    fn save_grid(&mut self, deck: usize, bpm: f64, first: f64) {
        let Some(track) = self.dj.decks.track(deck) else { return };
        let revision = self.dj.decks.snapshot["revision"].as_u64().unwrap_or(0);
        let params = json!({"track": track, "revision": revision, "bpm": bpm, "firstBeat": first.max(0.0)});
        if self.deck_command("dj.grid.set", params) {
            // Shown at once; the library's next read confirms it.
            let grids = &mut self.dj.decks.snapshot["grids"];
            if !grids.is_array() {
                *grids = json!([]);
            }
            if let Some(list) = grids.as_array_mut() {
                list.retain(|g| g["track"].as_u64() != Some(track));
                list.push(json!({"track": track, "bpm": bpm, "first_beat": first.max(0.0)}));
            }
        }
    }

    /// The key and beat row: key lock, key down / up / match, quantize, tap and the beat grid.
    pub(super) fn key_row(&mut self, ui: &mut Ui, deck: usize, ready: bool) {
        let name = NAMES[deck];
        let color = deck_color(deck);
        let d = self.dj.decks.deck(deck);
        let locked = d["keyLock"] == true;
        let shift = d["keyShift"].as_f64().unwrap_or(0.0);
        let known = d["key"].as_str().is_some_and(|k| Key::parse(k).is_some());
        let shown = self.dj.decks.shown_key(deck).map_or_else(|| "—".to_owned(), Key::text);
        let moved = self.dj.decks.key_moved(deck);
        let timed = ready && self.dj.decks.beat(deck).is_some();
        let quantize = self.dj.decks.quantize[deck];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if small_button(ui, &format!("Key lock {name}"), "KEY LOCK", locked, color, ready)
                .on_hover_text("Keep the key when the tempo changes")
                .clicked()
            {
                self.toggle_key_lock(deck);
            }
            if small_button(ui, &format!("Key down {name}"), "KEY −", false, color, ready)
                .on_hover_text("Key down a semitone")
                .clicked()
            {
                self.move_key(deck, -1);
            }
            let text = if moved.abs() >= 0.05 { format!("{shown} {moved:+.1}") } else { shown };
            let key = small_button(ui, &format!("Key {name}"), &text, shift != 0.0, color, ready)
                .on_hover_text("The key as it sounds now · Click: back to the track's own key");
            if key.clicked() {
                self.move_key(deck, 0);
            }
            if small_button(ui, &format!("Key up {name}"), "+", false, color, ready)
                .on_hover_text("Key up a semitone")
                .clicked()
            {
                self.move_key(deck, 1);
            }
            let other_known = self.dj.decks.shown_key(self.dj.decks.partner(deck)).is_some();
            if small_button(ui, &format!("Key sync {name}"), "KEY SYNC", false, color, ready && known && other_known)
                .on_hover_text("Move the key to match the other deck's")
                .clicked()
            {
                self.key_sync(deck);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let grid = small_button(ui, &format!("Beat grid {name}"), "GRID", false, color, timed)
                    .on_hover_text("Move the beat grid or change its tempo");
                egui::Popup::menu(&grid).show(|ui| {
                    if ui.button("Put the first beat here").clicked() {
                        self.grid_here(deck);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("◀ Earlier").clicked() {
                            self.move_grid(deck, -1.0);
                        }
                        if ui.button("Later ▶").clicked() {
                            self.move_grid(deck, 1.0);
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("− 0.1 BPM").clicked() {
                            self.grid_tempo(deck, -0.1);
                        }
                        if ui.button("+ 0.1 BPM").clicked() {
                            self.grid_tempo(deck, 0.1);
                        }
                    });
                });
                if small_button(ui, &format!("Tap tempo {name}"), "TAP", false, color, ready)
                    .on_hover_text("Tap along with the beat (4 times or more) to set the track's tempo")
                    .clicked()
                {
                    self.tap(deck);
                }
                if small_button(ui, &format!("Quantize {name}"), "QUANT", quantize, color, true)
                    .on_hover_text("Cues and loops land on the beat; jumps keep the beat")
                    .clicked()
                {
                    self.toggle_quantize(deck);
                }
            });
        });
    }
}

#[cfg(test)]
mod key_tests {
    use super::Key;

    #[test]
    fn keys_are_read_and_written_in_both_notations() {
        let am = Key::parse("Am").unwrap();
        assert_eq!((am.tonic, am.minor), (9, true));
        assert_eq!(Key::parse("8A").unwrap().text(), "8A");
        assert_eq!(Key::parse("8A").unwrap().tonic, 9, "8A is A minor");
        assert_eq!(Key::parse("8B").unwrap().tonic, 0, "8B is C major");
        assert_eq!(Key::parse("5A").unwrap().tonic, 0, "5A is C minor");
        assert_eq!(Key::parse("12B").unwrap().tonic, 4, "12B is E major");
        for n in 1..=12 {
            for side in ["A", "B"] {
                let code = format!("{n}{side}");
                assert_eq!(Key::parse(&code).unwrap().text(), code);
            }
        }
        assert_eq!(Key::parse("F# minor").unwrap().text(), "F#m");
        assert_eq!(Key::parse("Bb").unwrap().text(), "Bb");
        assert_eq!(Key::parse("Am").unwrap().moved(2).text(), "Bm");
        assert_eq!(Key::parse("8A").unwrap().moved(7).text(), "9A", "a fifth up is one step round the wheel");
        assert!(Key::parse("H").is_none());
        assert!(Key::parse("13A").is_none());
        assert!(Key::parse("").is_none());
    }
}
