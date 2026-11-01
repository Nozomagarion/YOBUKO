// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Restricted LAN MIDI/audio sessions: portable snapshots, verified sound files and ordered musical edits.

mod assets;
mod cache;
#[cfg(test)]
mod faults;
mod invitation;
mod linked;
mod midi;
mod online;
mod protocol;
mod resume;
mod sound;
mod starter;
mod transfer;
mod transport;

pub use assets::{AudioAddition, SharedSnapshot};
pub use linked::{LEAD_MS, SharedTransport, TransportRequest, beat_after, starts_in_ms};
pub use protocol::{Controls, Operation, TrackControl, WireEdit};
pub use starter::starter_project;
pub use transport::{MAX_GUESTS, ONLINE, ONLINE_LOOPBACK, Service, commands, suggested_address};
