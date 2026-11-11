// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Pioneer DDJ-FLX4 driving YOBUKO's decks. While the DJ space shows, the controller's
//! MIDI port is captured (`suhina_session::LiveInput::capture`): what it sends comes here as it
//! came and no longer plays an armed track. The MIDI numbers are those of the open-source
//! DDJ-FLX4 mapping for Mixxx by Robert904 and ElHanko (MIT), vendored in
//! `third_party/mixxx-ddj-flx4-mapping` (`docs/spec/dj-workspace.md` › The DDJ-FLX4).

/// What a message of the controller asks for. Decks are 0 (left, A) and 1 (right, B); levels
/// and knobs are 0 … 1 from the most significant 7 bits of their 14.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Flx4 {
    Play(usize),
    Cue(usize),
    Sync(usize),
    /// The CUE button of a mixer channel: that deck in the headphones.
    Phones(usize),
    Load(usize),
    /// The browse knob turned by this many steps, down the list when positive.
    Browse(i32),
    Level(usize, f32),
    /// Low, mid or high (0, 1, 2) of a deck's EQ.
    Eq(usize, usize, f32),
    Filter(usize, f32),
    /// The tempo fader, 0 … 1 from one end to the other.
    Tempo(usize, f32),
    Crossfader(f32),
    /// A jog wheel turned by this many steps (positive forwards), on its top or its side.
    Jog(usize, i32),
    /// The deck's SHIFT button, pressed (true) or let go.
    Shift(usize, bool),
    /// SHIFT and a mixer channel's CUE: a tap of the tempo.
    Tap(usize),
    /// SHIFT and CUE: back to the start of the track.
    ToStart(usize),
    Trim(usize, f32),
    /// The headphones' CUE/MASTER knob, 0 cue … 1 master.
    PhonesMix(f32),
    LoopIn(usize),
    LoopOut(usize),
    /// RELOOP/EXIT: lets a playing loop go, or plays the last loop again.
    ReloopExit(usize),
    /// CUE/LOOP CALL ◀ and ▶: the loop halved or doubled (`false`, `true`).
    LoopCall(usize, bool),
    /// A pad in HOT CUE mode, 0 … 7; with SHIFT held on the controller, it deletes.
    HotCue(usize, usize, bool),
    /// A pad in BEAT LOOP mode: a loop of this many beats, or the loop let go.
    BeatLoop(usize, f64),
    /// A pad in BEAT JUMP mode, or the quick jumps: this many beats, back when negative.
    BeatJump(usize, f64),
    /// A pad in SAMPLER mode, 0 … 7: that sampler slot played; with SHIFT, stopped.
    Sampler(usize, bool),
    /// BEAT FX SELECT: the next effect (1), or with SHIFT the one before (−1).
    FxSelect(i32),
    /// BEAT ◀ and ▶: the effect's length shorter (−1) or longer (1).
    FxBeats(i32),
    /// BEAT FX ON/OFF, and with SHIFT every effect of the unit off.
    FxOn,
    FxAllOff,
    /// The BEAT FX channel switch: on (true) or off deck A (0) or B (1).
    FxChannel(usize, bool),
    /// The LEVEL/DEPTH knob, 0 … 1.
    FxLevel(f32),
}

/// The beat loops of the BEAT LOOP pads, left to right, top to bottom.
const BEAT_LOOPS: [f64; 8] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0];
/// The beat jumps of the BEAT JUMP pads.
const BEAT_JUMPS: [f64; 8] = [-1.0, 1.0, -2.0, 2.0, -4.0, 4.0, -32.0, 32.0];

/// A deck's pads: notes on channel 8 (deck A) or 10 (deck B), on the next channel with SHIFT.
fn pad(channel: usize, number: u8) -> Option<Flx4> {
    let (deck, shifted) = match channel {
        7 => (0, false),
        8 => (0, true),
        9 => (1, false),
        10 => (1, true),
        _ => return None,
    };
    let index = usize::from(number & 0x07);
    match (number & 0xF8, shifted) {
        (0x00, _) => Some(Flx4::HotCue(deck, index, shifted)),
        (0x30, _) => Some(Flx4::Sampler(index, shifted)),
        (0x60, false) => BEAT_LOOPS.get(index).map(|beats| Flx4::BeatLoop(deck, *beats)),
        (0x20, false) => BEAT_JUMPS.get(index).map(|beats| Flx4::BeatJump(deck, *beats)),
        _ => None,
    }
}

/// Whether a MIDI port is a DDJ-FLX4, by its name.
pub(super) fn is_flx4(port: &str) -> bool {
    let name = port.to_ascii_uppercase();
    name.contains("DDJ-FLX4") || name.contains("DDJ FLX4") || name.contains("FLX4")
}

/// Reads one message; `None` for a button let go and anything the decks do not use.
pub(super) fn read(message: [u8; 3]) -> Option<Flx4> {
    let [status, number, value] = message;
    let channel = usize::from(status & 0x0F);
    let level = f32::from(value) / 127.0;
    match status & 0xF0 {
        // Buttons: on when pressed (velocity above 0), and a deck's are on its own channel.
        // SHIFT is held: it is read when let go too.
        0x90 if number == 0x3F && channel < 2 => Some(Flx4::Shift(channel, value > 0)),
        0x90 if value > 0 && (7..=10).contains(&channel) => pad(channel, number),
        // The BEAT FX channel switch says when it leaves a position too.
        0x90 if (channel, number) == (4, 0x10) => Some(Flx4::FxChannel(0, value > 0)),
        0x90 if (channel, number) == (5, 0x11) => Some(Flx4::FxChannel(1, value > 0)),
        0x90 if value > 0 => match (channel, number) {
            (0 | 1, 0x0B) => Some(Flx4::Play(channel)),
            (0 | 1, 0x0C) => Some(Flx4::Cue(channel)),
            (0 | 1, 0x58) => Some(Flx4::Sync(channel)),
            (0 | 1, 0x54) => Some(Flx4::Phones(channel)),
            (0 | 1, 0x68) => Some(Flx4::Tap(channel)),
            (0 | 1, 0x48) => Some(Flx4::ToStart(channel)),
            (0 | 1, 0x10) => Some(Flx4::LoopIn(channel)),
            (0 | 1, 0x11) => Some(Flx4::LoopOut(channel)),
            (0 | 1, 0x4D | 0x50) => Some(Flx4::ReloopExit(channel)),
            (0 | 1, 0x51) => Some(Flx4::LoopCall(channel, false)),
            (0 | 1, 0x53) => Some(Flx4::LoopCall(channel, true)),
            (0 | 1, 0x3E) => Some(Flx4::BeatJump(channel, -16.0)),
            (0 | 1, 0x3D) => Some(Flx4::BeatJump(channel, 16.0)),
            (6, 0x46) => Some(Flx4::Load(0)),
            (6, 0x47) => Some(Flx4::Load(1)),
            (4, 0x63) => Some(Flx4::FxSelect(1)),
            (4, 0x64) => Some(Flx4::FxSelect(-1)),
            (4, 0x4A) => Some(Flx4::FxBeats(-1)),
            (4, 0x4B) => Some(Flx4::FxBeats(1)),
            (4 | 5, 0x47) => Some(Flx4::FxOn),
            (4 | 5, 0x43) => Some(Flx4::FxAllOff),
            _ => None,
        },
        0xB0 => match (channel, number) {
            (0 | 1, 0x13) => Some(Flx4::Level(channel, level)),
            (0 | 1, 0x0F) => Some(Flx4::Eq(channel, 0, level)),
            (0 | 1, 0x0B) => Some(Flx4::Eq(channel, 1, level)),
            (0 | 1, 0x07) => Some(Flx4::Eq(channel, 2, level)),
            (0 | 1, 0x00) => Some(Flx4::Tempo(channel, level)),
            (0 | 1, 0x04) => Some(Flx4::Trim(channel, level)),
            // The side of the wheel, and its top (touched or not): 64 is still.
            (0 | 1, 0x21..=0x23) => Some(Flx4::Jog(channel, i32::from(value) - 64)),
            (6, 0x17) => Some(Flx4::Filter(0, level)),
            (6, 0x18) => Some(Flx4::Filter(1, level)),
            (6, 0x1F) => Some(Flx4::Crossfader(level)),
            (6, 0x0C) => Some(Flx4::PhonesMix(level)),
            (4 | 5, 0x02) => Some(Flx4::FxLevel(level)),
            // Relative: 1 … 63 down the list, 65 … 127 up it.
            (6, 0x40) => Some(Flx4::Browse(if value < 64 { i32::from(value) } else { i32::from(value) - 128 })),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flx4_buttons_faders_and_knobs_are_read_per_deck() {
        assert_eq!(read([0x90, 0x0B, 0x7F]), Some(Flx4::Play(0)));
        assert_eq!(read([0x91, 0x0B, 0x7F]), Some(Flx4::Play(1)));
        assert_eq!(read([0x90, 0x0B, 0x00]), None, "a button let go");
        assert_eq!(read([0x91, 0x0C, 0x7F]), Some(Flx4::Cue(1)));
        assert_eq!(read([0x90, 0x58, 0x7F]), Some(Flx4::Sync(0)));
        assert_eq!(read([0x91, 0x54, 0x7F]), Some(Flx4::Phones(1)));
        assert_eq!(read([0x90, 0x68, 0x7F]), Some(Flx4::Tap(0)));
        assert_eq!(read([0x97, 0x32, 0x7F]), Some(Flx4::Sampler(2, false)));
        assert_eq!(read([0x9A, 0x37, 0x7F]), Some(Flx4::Sampler(7, true)));
        assert_eq!(read([0x94, 0x63, 0x7F]), Some(Flx4::FxSelect(1)));
        assert_eq!(read([0x94, 0x64, 0x7F]), Some(Flx4::FxSelect(-1)));
        assert_eq!(read([0x94, 0x4B, 0x7F]), Some(Flx4::FxBeats(1)));
        assert_eq!(read([0x95, 0x47, 0x7F]), Some(Flx4::FxOn));
        assert_eq!(read([0x94, 0x43, 0x7F]), Some(Flx4::FxAllOff));
        assert_eq!(read([0x94, 0x10, 0x7F]), Some(Flx4::FxChannel(0, true)));
        assert_eq!(read([0x95, 0x11, 0x00]), Some(Flx4::FxChannel(1, false)));
        assert_eq!(read([0xB4, 0x02, 0x7F]), Some(Flx4::FxLevel(1.0)));
        assert_eq!(read([0x96, 0x46, 0x7F]), Some(Flx4::Load(0)));
        assert_eq!(read([0x96, 0x47, 0x7F]), Some(Flx4::Load(1)));
        assert_eq!(read([0xB0, 0x13, 0x7F]), Some(Flx4::Level(0, 1.0)));
        assert_eq!(read([0xB1, 0x13, 0x00]), Some(Flx4::Level(1, 0.0)));
        assert_eq!(read([0xB0, 0x0F, 0x00]), Some(Flx4::Eq(0, 0, 0.0)));
        assert_eq!(read([0xB1, 0x0B, 0x7F]), Some(Flx4::Eq(1, 1, 1.0)));
        assert_eq!(read([0xB0, 0x07, 0x7F]), Some(Flx4::Eq(0, 2, 1.0)));
        assert_eq!(read([0xB6, 0x17, 0x00]), Some(Flx4::Filter(0, 0.0)));
        assert_eq!(read([0xB6, 0x18, 0x7F]), Some(Flx4::Filter(1, 1.0)));
        assert_eq!(read([0xB6, 0x1F, 0x7F]), Some(Flx4::Crossfader(1.0)));
        assert_eq!(read([0xB0, 0x00, 0x00]), Some(Flx4::Tempo(0, 0.0)));
        assert_eq!(read([0xB6, 0x40, 0x01]), Some(Flx4::Browse(1)));
        assert_eq!(read([0xB6, 0x40, 0x7F]), Some(Flx4::Browse(-1)));
        assert_eq!(read([0xB0, 0x22, 0x41]), Some(Flx4::Jog(0, 1)));
        assert_eq!(read([0xB1, 0x21, 0x3E]), Some(Flx4::Jog(1, -2)));
        assert_eq!(read([0x90, 0x3F, 0x7F]), Some(Flx4::Shift(0, true)));
        assert_eq!(read([0x91, 0x3F, 0x00]), Some(Flx4::Shift(1, false)));
        assert_eq!(read([0x90, 0x48, 0x7F]), Some(Flx4::ToStart(0)));
        assert_eq!(read([0xB1, 0x04, 0x7F]), Some(Flx4::Trim(1, 1.0)));
        assert_eq!(read([0xB6, 0x0C, 0x00]), Some(Flx4::PhonesMix(0.0)));
        assert_eq!(read([0x90, 0x10, 0x7F]), Some(Flx4::LoopIn(0)));
        assert_eq!(read([0x91, 0x11, 0x7F]), Some(Flx4::LoopOut(1)));
        assert_eq!(read([0x90, 0x4D, 0x7F]), Some(Flx4::ReloopExit(0)));
        assert_eq!(read([0x91, 0x53, 0x7F]), Some(Flx4::LoopCall(1, true)));
        assert_eq!(read([0x97, 0x00, 0x7F]), Some(Flx4::HotCue(0, 0, false)));
        assert_eq!(read([0x9A, 0x07, 0x7F]), Some(Flx4::HotCue(1, 7, true)));
        assert_eq!(read([0x97, 0x00, 0x00]), None, "a pad let go");
        assert_eq!(read([0x99, 0x64, 0x7F]), Some(Flx4::BeatLoop(1, 4.0)));
        assert_eq!(read([0x97, 0x26, 0x7F]), Some(Flx4::BeatJump(0, -32.0)));
        assert_eq!(read([0x90, 0x3D, 0x7F]), Some(Flx4::BeatJump(0, 16.0)));
        // Keyboard, pad FX and pitch pads are not the decks' yet.
        for later in [[0x97, 0x40, 0x7F], [0x97, 0x10, 0x7F], [0x99, 0x70, 0x7F]] {
            assert_eq!(read(later), None, "{later:02x?}");
        }
        // The least significant halves of 14-bit controls, and other channels, are left alone.
        for unused in [[0xB0, 0x33, 0x10], [0xB0, 0x20, 0x10], [0x92, 0x0B, 0x7F], [0xE0, 0, 64], [0, 0, 0]] {
            assert_eq!(read(unused), None, "{unused:02x?}");
        }
    }

    #[test]
    fn the_flx4_is_known_by_its_port_name() {
        assert!(is_flx4("DDJ-FLX4 MIDI 1"));
        assert!(is_flx4("DDJ-FLX4:DDJ-FLX4 MIDI 1 24:0"));
        assert!(is_flx4("ddj-flx4"));
        assert!(!is_flx4("Launchkey Mini MK3"));
    }
}
