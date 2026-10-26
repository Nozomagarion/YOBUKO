// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Restricted LAN MIDI sessions: bounded musical copies, track controls and single-note operations.

mod invitation;
mod midi;
mod protocol;
mod starter;
mod transfer;
mod transport;

pub use protocol::{Controls, Operation, TrackControl, WireEdit};
pub use starter::starter_project;
pub use transport::{Service, commands, suggested_address};
