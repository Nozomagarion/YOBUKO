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
        0x90 if value > 0 => match (channel, number) {
            (0 | 1, 0x0B) => Some(Flx4::Play(channel)),
            (0 | 1, 0x0C) => Some(Flx4::Cue(channel)),
            (0 | 1, 0x58) => Some(Flx4::Sync(channel)),
            (0 | 1, 0x54) => Some(Flx4::Phones(channel)),
            (6, 0x46) => Some(Flx4::Load(0)),
            (6, 0x47) => Some(Flx4::Load(1)),
            _ => None,
        },
        0xB0 => match (channel, number) {
            (0 | 1, 0x13) => Some(Flx4::Level(channel, level)),
            (0 | 1, 0x0F) => Some(Flx4::Eq(channel, 0, level)),
            (0 | 1, 0x0B) => Some(Flx4::Eq(channel, 1, level)),
            (0 | 1, 0x07) => Some(Flx4::Eq(channel, 2, level)),
            (0 | 1, 0x00) => Some(Flx4::Tempo(channel, level)),
            // The side of the wheel, and its top (touched or not): 64 is still.
            (0 | 1, 0x21..=0x23) => Some(Flx4::Jog(channel, i32::from(value) - 64)),
            (6, 0x17) => Some(Flx4::Filter(0, level)),
            (6, 0x18) => Some(Flx4::Filter(1, level)),
            (6, 0x1F) => Some(Flx4::Crossfader(level)),
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
