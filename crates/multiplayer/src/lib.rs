// Copyright (C) 2026 The Suhina contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! LAN starter sessions. The wire carries track controls, never arbitrary projects or files.

mod invitation;
mod midi;
mod protocol;
mod starter;
mod transfer;
mod transport;

pub use protocol::{Controls, TrackControl};
pub use starter::starter_project;
pub use transport::{Service, commands, suggested_address};
